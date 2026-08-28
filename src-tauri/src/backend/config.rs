//! 路径与目录配置。
//!
//! - 数据目录：%APPDATA%/uc-drive2（env UC_DRIVE2_DATA_DIR 可覆盖，测试/开发用）
//! - 存储根目录：默认 %APPDATA%/uc-drive2/storage（env UC_DRIVE2_STORAGE_DIR 可覆盖），
//!   运行时可由设置接口切换（Arc<RwLock> 全局态）。
//! - 所有路径统一正斜杠存 DB（norm_path），FS 操作用原生 PathBuf。

use std::path::PathBuf;
use std::sync::{Arc, RwLock};

#[derive(Debug, Clone)]
pub struct Paths {
    pub data_dir: PathBuf,
    pub gopeed_dir: PathBuf,
    pub db_dir: PathBuf,
    pub db_file: PathBuf,
    pub port_file: PathBuf,
}

impl Paths {
    pub fn new(data_dir: &std::path::Path) -> Self {
        Paths {
            data_dir: data_dir.to_path_buf(),
            gopeed_dir: data_dir.join("gopeed"),
            db_dir: data_dir.join("data"),
            db_file: data_dir.join("data").join("uc-drive.db"),
            port_file: data_dir.join("server.port"),
        }
    }
}

/// 数据目录解析：env UC_DRIVE2_DATA_DIR > %APPDATA%/uc-drive2 > ~/.uc-drive2
pub fn resolve_data_dir() -> PathBuf {
    if let Ok(d) = std::env::var("UC_DRIVE2_DATA_DIR") {
        return PathBuf::from(d);
    }
    if let Ok(appdata) = std::env::var("APPDATA") {
        return PathBuf::from(appdata).join("uc-drive2");
    }
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".uc-drive2")
}

/// 网盘存储目录 env 覆盖（测试/开发用），无则 None
pub fn storage_dir_override() -> Option<PathBuf> {
    std::env::var("UC_DRIVE2_STORAGE_DIR")
        .ok()
        .map(PathBuf::from)
}

/// 默认存储根目录（= storage_dir 覆盖值，否则 data_dir/storage）
pub fn default_storage_dir(
    data_dir: &std::path::Path,
    override_dir: Option<&std::path::Path>,
) -> PathBuf {
    match override_dir {
        Some(p) => p.to_path_buf(),
        None => data_dir.join("storage"),
    }
}

/// 存储根目录的运行时共享句柄。
#[derive(Clone)]
pub struct Storage(pub Arc<RwLock<PathBuf>>);

impl Storage {
    pub fn new(dir: PathBuf) -> Self {
        Storage(Arc::new(RwLock::new(dir)))
    }

    pub fn get(&self) -> PathBuf {
        self.0.read().unwrap().clone()
    }

    pub fn set(&self, dir: PathBuf) {
        *self.0.write().unwrap() = dir;
    }

    /// 离线下载暂存目录（storage/offline）
    pub fn offline_dir(&self) -> PathBuf {
        self.get().join("offline")
    }
}
