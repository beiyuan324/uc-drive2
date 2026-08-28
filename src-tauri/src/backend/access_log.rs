//! 访问日志缓冲：每 3 秒批量写入 data/access.log，避免高频轮询逐请求同步 IO。

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::util::now_iso;

#[derive(Clone)]
pub struct AccessLog {
    path: PathBuf,
    buffer: Arc<Mutex<Vec<String>>>,
}

impl AccessLog {
    pub fn new(path: PathBuf) -> Self {
        AccessLog {
            path,
            buffer: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn push(&self, method: &str, url: &str) {
        let mut buffer = self.buffer.lock().unwrap();
        buffer.push(format!("{} {} {}\n", now_iso(), method, url));
        if buffer.len() >= 1024 {
            let chunk = buffer.drain(..).collect::<String>();
            drop(buffer);
            let _ = append(&self.path, &chunk);
        }
    }

    pub fn spawn_flusher(&self) {
        let this = self.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(3));
            loop {
                interval.tick().await;
                this.flush();
            }
        });
    }

    pub fn flush(&self) {
        let chunk = {
            let mut buffer = self.buffer.lock().unwrap();
            if buffer.is_empty() {
                return;
            }
            buffer.drain(..).collect::<String>()
        };
        let _ = append(&self.path, &chunk);
    }
}

fn append(path: &PathBuf, chunk: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    file.write_all(chunk.as_bytes())
}
