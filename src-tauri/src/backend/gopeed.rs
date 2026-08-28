//! gopeed-web 进程管理器。
//! Rust 后端负责进程托管与 REST 客户端；gopeed 本身仍作为唯一下载引擎保留。

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use reqwest::Method;
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::broadcast;

use super::util::{encode_uri_component, now_ms};
use super::winutil::{terminate_pid, CREATE_NO_WINDOW};

const HOST: &str = "127.0.0.1";
const MAX_RESTARTS: u32 = 5;

#[derive(Debug, Clone)]
pub enum GopeedEvent {
    Started,
    Exited { code: Option<i32> },
}

/// 可注入的进程启动器：生产环境使用 RealGopeedSpawner，测试可提供 mock。
pub trait GopeedSpawner: Send + Sync + 'static {
    fn spawn(
        &self,
        host: &str,
        port: u16,
        token: &str,
        storage_dir: &Path,
    ) -> std::io::Result<Child>;
}

#[derive(Debug, Clone)]
pub struct RealGopeedSpawner {
    pub exe_path: PathBuf,
}

impl GopeedSpawner for RealGopeedSpawner {
    fn spawn(
        &self,
        host: &str,
        port: u16,
        token: &str,
        storage_dir: &Path,
    ) -> std::io::Result<Child> {
        let mut command = Command::new(&self.exe_path);
        command
            .args([
                "-A",
                host,
                "-P",
                &port.to_string(),
                "-T",
                token,
                "-d",
                &storage_dir.to_string_lossy(),
            ])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        #[cfg(windows)]
        {
            command.creation_flags(CREATE_NO_WINDOW);
        }
        command.spawn()
    }
}

struct ManagerInner {
    pid: Option<u32>,
    port: Option<u16>,
    base: Option<String>,
    stopping: bool,
    restarts: u32,
}

/// gopeed 进程 + REST 客户端。所有状态通过 Mutex 保护，异步请求不会持锁。
pub struct GopeedManager {
    exe_path: PathBuf,
    storage_dir: PathBuf,
    token: String,
    spawner: Arc<dyn GopeedSpawner>,
    client: reqwest::Client,
    inner: Mutex<ManagerInner>,
    events: broadcast::Sender<GopeedEvent>,
    pid_sink: Option<Arc<Mutex<Option<u32>>>>,
}

impl std::fmt::Debug for GopeedManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GopeedManager")
            .field("exe_path", &self.exe_path)
            .field("storage_dir", &self.storage_dir)
            .field("token", &"<redacted>")
            .field("port", &self.port())
            .field("base", &self.base())
            .finish()
    }
}

impl GopeedManager {
    pub fn new(
        exe_path: PathBuf,
        storage_dir: PathBuf,
        pid_sink: Option<Arc<Mutex<Option<u32>>>>,
    ) -> Arc<Self> {
        Self::with_spawner(
            exe_path.clone(),
            storage_dir,
            Arc::new(RealGopeedSpawner { exe_path }),
            pid_sink,
        )
    }

    pub fn with_spawner(
        exe_path: PathBuf,
        storage_dir: PathBuf,
        spawner: Arc<dyn GopeedSpawner>,
        pid_sink: Option<Arc<Mutex<Option<u32>>>>,
    ) -> Arc<Self> {
        let (events, _) = broadcast::channel(32);
        let mut bytes = [0u8; 16];
        use rand::RngCore;
        rand::thread_rng().fill_bytes(&mut bytes);
        let token = bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
        Arc::new(GopeedManager {
            exe_path,
            storage_dir,
            token,
            spawner,
            // 本地回环必须直连：reqwest 默认启用 system-proxy，会读取 Windows 系统代理
            // （如 Clash 127.0.0.1:7897），把对 gopeed 的 127.0.0.1 请求转发给代理导致 502/超时。
            client: reqwest::Client::builder()
                .no_proxy()
                .build()
                .expect("reqwest client"),
            inner: Mutex::new(ManagerInner {
                pid: None,
                port: None,
                base: None,
                stopping: false,
                restarts: 0,
            }),
            events,
            pid_sink,
        })
    }

    pub fn token(&self) -> String {
        self.token.clone()
    }

    pub fn subscribe(&self) -> broadcast::Receiver<GopeedEvent> {
        self.events.subscribe()
    }

    pub fn running(&self) -> bool {
        self.inner.lock().unwrap().pid.is_some()
    }

    pub fn ready(&self) -> bool {
        let inner = self.inner.lock().unwrap();
        inner.pid.is_some() && inner.base.is_some()
    }

    pub fn pid(&self) -> Option<u32> {
        self.inner.lock().unwrap().pid
    }

    pub fn port(&self) -> Option<u16> {
        self.inner.lock().unwrap().port
    }

    pub fn base(&self) -> Option<String> {
        self.inner.lock().unwrap().base.clone()
    }

    /// 启动 gopeed 并等待 REST 健康检查。
    pub async fn start(self: &Arc<Self>) -> Result<(), String> {
        if self.running() {
            return Ok(());
        }
        {
            let mut inner = self.inner.lock().unwrap();
            inner.stopping = false;
        }
        let port = free_port().map_err(|e| e.to_string())?;
        self.set_endpoint(port);
        // 缺少可执行文件时记录并触发重试；上层通常会在启动前检查 exists。
        let _ = self.spawn_process(port);
        self.wait_ready(Duration::from_secs(30)).await?;
        let _ = self.events.send(GopeedEvent::Started);
        Ok(())
    }

    fn set_endpoint(&self, port: u16) {
        let mut inner = self.inner.lock().unwrap();
        inner.port = Some(port);
        inner.base = Some(format!("http://{HOST}:{port}"));
    }

    fn spawn_process(self: &Arc<Self>, port: u16) -> Result<(), String> {
        let child = match self
            .spawner
            .spawn(HOST, port, &self.token, &self.storage_dir)
        {
            Ok(child) => child,
            Err(err) => {
                eprintln!("[gopeed] 启动失败: {err}");
                self.mark_exited(None);
                self.schedule_restart();
                return Err(err.to_string());
            }
        };
        let pid = child.id();
        {
            let mut inner = self.inner.lock().unwrap();
            inner.pid = pid;
        }
        if let Some(sink) = &self.pid_sink {
            *sink.lock().unwrap() = pid;
        }

        let mut child = child;
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        if let Some(stdout) = stdout {
            let mut lines = BufReader::new(stdout).lines();
            tokio::spawn(async move {
                while let Ok(Some(line)) = lines.next_line().await {
                    if !line.trim().is_empty() {
                        eprintln!("[gopeed] {}", line.trim());
                    }
                }
            });
        }
        if let Some(stderr) = stderr {
            let mut lines = BufReader::new(stderr).lines();
            tokio::spawn(async move {
                while let Ok(Some(line)) = lines.next_line().await {
                    if !line.trim().is_empty() {
                        eprintln!("[gopeed:err] {}", line.trim());
                    }
                }
            });
        }

        let weak = Arc::downgrade(self);
        tokio::spawn(async move {
            let status = child.wait().await.ok();
            if let Some(manager) = weak.upgrade() {
                manager.child_exited(status.and_then(|s| s.code())).await;
            }
        });
        Ok(())
    }

    fn mark_exited(&self, code: Option<i32>) {
        {
            let mut inner = self.inner.lock().unwrap();
            inner.pid = None;
            inner.base = None;
        }
        if let Some(sink) = &self.pid_sink {
            *sink.lock().unwrap() = None;
        }
        let _ = self.events.send(GopeedEvent::Exited { code });
    }

    async fn child_exited(self: Arc<Self>, code: Option<i32>) {
        let should_restart = {
            let mut inner = self.inner.lock().unwrap();
            let stopping = inner.stopping;
            inner.pid = None;
            inner.base = None;
            !stopping && inner.restarts < MAX_RESTARTS
        };
        if let Some(sink) = &self.pid_sink {
            *sink.lock().unwrap() = None;
        }
        let _ = self.events.send(GopeedEvent::Exited { code });
        if should_restart {
            self.schedule_restart();
        } else if !self.inner.lock().unwrap().stopping {
            eprintln!("[gopeed] 重启次数超限，放弃");
        }
    }

    fn schedule_restart(self: &Arc<Self>) {
        let should_schedule = {
            let mut inner = self.inner.lock().unwrap();
            if inner.stopping || inner.restarts >= MAX_RESTARTS {
                false
            } else {
                inner.restarts += 1;
                eprintln!(
                    "[gopeed] 异常退出，1.5s 后重启 ({}/{})",
                    inner.restarts, MAX_RESTARTS
                );
                true
            }
        };
        if !should_schedule {
            return;
        }
        let weak: Weak<Self> = Arc::downgrade(self);
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(1500)).await;
            let Some(manager) = weak.upgrade() else {
                return;
            };
            if manager.inner.lock().unwrap().stopping {
                return;
            }
            let Ok(port) = free_port() else { return };
            manager.set_endpoint(port);
            if manager.spawn_process(port).is_ok() {
                if manager.wait_ready(Duration::from_secs(30)).await.is_ok() {
                    let _ = manager.events.send(GopeedEvent::Started);
                } else {
                    eprintln!("[gopeed] 重启后健康检查失败");
                }
            }
        });
    }

    /// 重启：换新端口、重新拉起、等待健康。
    pub async fn restart(self: &Arc<Self>) -> Result<(), String> {
        let port = free_port().map_err(|e| e.to_string())?;
        self.set_endpoint(port);
        self.spawn_process(port)?;
        self.wait_ready(Duration::from_secs(30)).await?;
        let _ = self.events.send(GopeedEvent::Started);
        Ok(())
    }

    pub async fn wait_ready(&self, timeout: Duration) -> Result<(), String> {
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            if tokio::time::Instant::now() >= deadline {
                return Err("gopeed 启动超时".to_string());
            }
            if !self.running() {
                tokio::time::sleep(Duration::from_millis(200)).await;
                continue;
            }
            if self.info().await.is_ok() {
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(400)).await;
        }
    }

    /// 停止 gopeed；Tauri 退出路径也会直接按 pid 终止，避免等待 WebView 清理。
    pub async fn stop(&self) {
        {
            let mut inner = self.inner.lock().unwrap();
            inner.stopping = true;
        }
        if let Some(pid) = self.pid() {
            terminate_pid(pid);
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
        let mut inner = self.inner.lock().unwrap();
        inner.pid = None;
        inner.base = None;
        if let Some(sink) = &self.pid_sink {
            *sink.lock().unwrap() = None;
        }
    }

    // ---------- REST 客户端 ----------

    async fn req_json(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
    ) -> Result<Value, String> {
        let base = self.base().ok_or_else(|| "gopeed 未就绪".to_string())?;
        let mut request = self
            .client
            .request(method, format!("{base}{path}"))
            .header("X-Api-Token", &self.token);
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request.send().await.map_err(|e| e.to_string())?;
        let value: Value = response
            .json()
            .await
            .unwrap_or_else(|_| json!({ "code": -1, "msg": "bad response" }));
        let code = value.get("code").and_then(Value::as_i64).unwrap_or(-1);
        if code != 0 {
            return Err(value
                .get("msg")
                .and_then(Value::as_str)
                .unwrap_or("gopeed 请求失败")
                .to_string());
        }
        Ok(value.get("data").cloned().unwrap_or(Value::Null))
    }

    pub async fn info(&self) -> Result<Value, String> {
        self.req_json(Method::GET, "/api/v1/info", None).await
    }

    pub async fn get_config(&self) -> Result<Value, String> {
        self.req_json(Method::GET, "/api/v1/config", None).await
    }

    pub async fn put_config(&self, config: Value) -> Result<Value, String> {
        self.req_json(Method::PUT, "/api/v1/config", Some(config))
            .await
    }

    pub async fn create_task(&self, req: Value, opts: Value) -> Result<String, String> {
        let data = self
            .req_json(
                Method::POST,
                "/api/v1/tasks",
                Some(json!({ "req": req, "opts": opts })),
            )
            .await?;
        Ok(match data {
            Value::String(s) => s,
            other => other.to_string(),
        })
    }

    pub async fn list_tasks(&self) -> Result<Vec<Value>, String> {
        let data = self.req_json(Method::GET, "/api/v1/tasks", None).await?;
        Ok(data.as_array().cloned().unwrap_or_default())
    }

    pub async fn get_task(&self, id: &str) -> Result<Value, String> {
        self.req_json(
            Method::GET,
            &format!("/api/v1/tasks/{}", encode_uri_component(id)),
            None,
        )
        .await
    }

    pub async fn pause(&self, id: &str) -> Result<Value, String> {
        self.req_json(
            Method::PUT,
            &format!("/api/v1/tasks/{}/pause", encode_uri_component(id)),
            None,
        )
        .await
    }

    pub async fn resume(&self, id: &str) -> Result<Value, String> {
        self.req_json(
            Method::PUT,
            &format!("/api/v1/tasks/{}/continue", encode_uri_component(id)),
            None,
        )
        .await
    }

    pub async fn remove(&self, id: &str, force: bool) -> Result<Value, String> {
        self.req_json(
            Method::DELETE,
            &format!("/api/v1/tasks/{}?force={force}", encode_uri_component(id)),
            None,
        )
        .await
    }

    pub fn map_status(status: &str) -> &'static str {
        match status {
            "ready" | "running" => "running",
            "pause" => "paused",
            "wait" => "queued",
            "error" => "error",
            "done" => "done",
            _ => "queued",
        }
    }
}

/// 申请一个临时本地端口。调用者随后立即启动服务。
fn free_port() -> std::io::Result<u16> {
    let listener = std::net::TcpListener::bind((HOST, 0))?;
    Ok(listener.local_addr()?.port())
}

#[allow(dead_code)]
fn _timestamp_for_logs() -> i64 {
    now_ms()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 状态映射() {
        assert_eq!(GopeedManager::map_status("ready"), "running");
        assert_eq!(GopeedManager::map_status("pause"), "paused");
        assert_eq!(GopeedManager::map_status("wait"), "queued");
        assert_eq!(GopeedManager::map_status("error"), "error");
        assert_eq!(GopeedManager::map_status("done"), "done");
    }
}
