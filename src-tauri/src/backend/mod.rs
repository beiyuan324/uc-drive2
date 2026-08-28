//! Rust 后端装配与生命周期。
//! Tauri 进程内直接运行 axum；gopeed-web 仍作为唯一外部下载引擎。

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Router;
use rusqlite::Connection;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

pub mod access_log;
pub mod config;
pub mod crypto;
pub mod db;
pub mod error;
pub mod files;
pub mod gopeed;
pub mod models;
pub mod routes;
pub mod tasks;
pub mod uc;
pub mod util;
pub mod winutil;

use access_log::AccessLog;
use config::{default_storage_dir, Paths, Storage};
use crypto::get_setting;
use error::{ApiError, ApiResult};
use gopeed::{GopeedManager, GopeedSpawner};
use tasks::TaskService;

pub const BASE_PORT: u16 = 17210;
const PORT_TRIES: u16 = 20;

pub struct AppState {
    pub paths: Paths,
    pub default_storage: PathBuf,
    pub storage: Storage,
    pub db: Arc<Mutex<Connection>>,
    pub gopeed: Arc<GopeedManager>,
    pub tasks: Arc<TaskService>,
    pub client: reqwest::Client,
    pub access_log: AccessLog,
}

pub struct StartOptions {
    pub data_dir: PathBuf,
    /// None = data_dir/storage；Some = env UC_DRIVE2_STORAGE_DIR 语义。
    pub storage_dir: Option<PathBuf>,
    pub gopeed_exe: PathBuf,
    pub spawner: Option<Arc<dyn GopeedSpawner>>,
    pub pid_sink: Option<Arc<Mutex<Option<u32>>>>,
    pub base_port: u16,
    pub port_tries: u16,
    pub poll_interval: Duration,
}

impl StartOptions {
    pub fn new(data_dir: PathBuf, storage_dir: Option<PathBuf>, gopeed_exe: PathBuf) -> Self {
        StartOptions {
            data_dir,
            storage_dir,
            gopeed_exe,
            spawner: None,
            pid_sink: None,
            base_port: BASE_PORT,
            port_tries: PORT_TRIES,
            poll_interval: Duration::from_secs(2),
        }
    }
}

pub struct BackendHandle {
    pub port: u16,
    pub state: Arc<AppState>,
    server: JoinHandle<Result<(), std::io::Error>>,
}

impl BackendHandle {
    pub async fn wait(self) -> Result<(), String> {
        self.server
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())
    }
}

/// 创建状态但不启动 gopeed/server，便于测试直接使用 build_router。
pub fn build_state(options: &StartOptions) -> ApiResult<Arc<AppState>> {
    std::fs::create_dir_all(&options.data_dir).map_err(|e| ApiError::from_io(&e))?;
    let paths = Paths::new(&options.data_dir);
    std::fs::create_dir_all(&paths.db_dir).map_err(|e| ApiError::from_io(&e))?;
    let connection = db::open_db(&paths.db_file);
    let default_storage = default_storage_dir(&options.data_dir, options.storage_dir.as_deref());
    let saved_storage = get_setting(&connection, "storage_dir", "");
    let initial_storage = if saved_storage.is_empty() {
        default_storage.clone()
    } else {
        resolve_path(&saved_storage)
    };
    let storage = Storage::new(initial_storage);
    for directory in [
        options.data_dir.clone(),
        paths.gopeed_dir.clone(),
        storage.get(),
        storage.offline_dir(),
        options.data_dir.join("tmp"),
    ] {
        std::fs::create_dir_all(directory).map_err(|e| ApiError::from_io(&e))?;
    }
    let db = Arc::new(Mutex::new(connection));
    let gopeed = if let Some(spawner) = options.spawner.clone() {
        GopeedManager::with_spawner(
            options.gopeed_exe.clone(),
            paths.gopeed_dir.clone(),
            spawner,
            options.pid_sink.clone(),
        )
    } else {
        GopeedManager::new(
            options.gopeed_exe.clone(),
            paths.gopeed_dir.clone(),
            options.pid_sink.clone(),
        )
    };
    // UC/OSS 直连，不读系统代理：与旧 Node(undici) 行为一致，避免本地代理规则干扰下载直链。
    let client = reqwest::Client::builder()
        .no_proxy()
        .build()
        .expect("reqwest client");
    let tasks = TaskService::new(
        db.clone(),
        storage.clone(),
        paths.data_dir.clone(),
        gopeed.clone(),
        client.clone(),
        options.poll_interval,
    );
    let access_log = AccessLog::new(paths.data_dir.join("access.log"));
    Ok(Arc::new(AppState {
        paths,
        default_storage,
        storage,
        db,
        gopeed,
        tasks,
        client,
        access_log,
    }))
}

/// 启动完整后端：数据库恢复 → gopeed（可选）→ 绑定本地 HTTP → 写端口文件 → serve。
pub async fn start(options: StartOptions) -> ApiResult<BackendHandle> {
    let state = build_state(&options)?;
    state.tasks.spawn_loops();
    state.access_log.spawn_flusher();

    if options.gopeed_exe.exists() || options.spawner.is_some() {
        if let Err(err) = state.gopeed.start().await {
            eprintln!("[uc-drive2] gopeed 启动失败（离线下载不可用）: {err}");
        } else {
            eprintln!("[uc-drive2] gopeed 就绪: {:?}", state.gopeed.base());
        }
    } else {
        eprintln!(
            "[uc-drive2] 未找到 gopeed-web.exe（离线下载不可用）: {}",
            options.gopeed_exe.display()
        );
    }

    let (listener, port) = bind_port(options.base_port, options.port_tries)
        .await
        .map_err(|e| ApiError::internal(format!("端口不可用: {e}")))?;
    std::fs::write(&state.paths.port_file, port.to_string()).map_err(|e| ApiError::from_io(&e))?;
    eprintln!(
        "[uc-drive2] 后端已监听 http://127.0.0.1:{port}（写入 {}）",
        state.paths.port_file.display()
    );

    let router: Router = routes::build_router(state.clone());
    let server = tokio::spawn(async move { axum::serve(listener, router).await });
    Ok(BackendHandle {
        port,
        state,
        server,
    })
}

async fn bind_port(base: u16, tries: u16) -> std::io::Result<(TcpListener, u16)> {
    if base == 0 {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
        let port = listener.local_addr()?.port();
        return Ok((listener, port));
    }
    let tries = tries.max(1);
    for offset in 0..tries {
        let port = base.saturating_add(offset);
        match TcpListener::bind(("127.0.0.1", port)).await {
            Ok(listener) => return Ok((listener, port)),
            Err(err) if err.kind() == std::io::ErrorKind::AddrInUse && offset + 1 < tries => {}
            Err(err) => return Err(err),
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::AddrInUse,
        "没有可用端口",
    ))
}

fn resolve_path(raw: &str) -> PathBuf {
    let path = PathBuf::from(raw);
    if path.is_absolute() {
        path
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    }
}

/// 生产/开发环境解析 gopeed-web.exe 路径。
pub fn resolve_gopeed_path(resource_dir: Option<&Path>) -> PathBuf {
    if let Ok(path) = std::env::var("GOPEED_PATH") {
        let path = PathBuf::from(path);
        if path.exists() {
            return path;
        }
    }
    let mut candidates = Vec::new();
    if let Some(resource) = resource_dir {
        candidates.push(resource.join("_up_").join("gopeed-web.exe"));
        candidates.push(resource.join("gopeed-web.exe"));
        if let Ok(entries) = std::fs::read_dir(resource.join("_up_")) {
            for entry in entries.flatten() {
                if entry.path().is_dir() {
                    candidates.push(entry.path().join("gopeed-web.exe"));
                }
            }
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("bin").join("gopeed").join("gopeed-web.exe"));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            candidates.push(parent.join("gopeed-web.exe"));
        }
    }
    candidates
        .iter()
        .find(|path| path.exists())
        .cloned()
        .unwrap_or_else(|| {
            candidates
                .into_iter()
                .next()
                .unwrap_or_else(|| PathBuf::from("gopeed-web.exe"))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn 端口占用时递增并遵守尝试次数() {
        let occupied = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let base = occupied.local_addr().unwrap().port();
        let (_selected, port) = bind_port(base, 2).await.unwrap();
        assert_eq!(port, base + 1);
        drop(occupied);

        let occupied = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let base = occupied.local_addr().unwrap().port();
        let error = bind_port(base, 1).await.unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::AddrInUse);
    }
}
