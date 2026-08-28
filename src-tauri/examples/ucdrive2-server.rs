//! 独立后端开发入口：用于浏览器/Vite 纯 Web 调试，不依赖 Tauri 窗口。

use std::time::Duration;

use uc_drive2_lib::backend;
use uc_drive2_lib::backend::config::{resolve_data_dir, storage_dir_override};

#[tokio::main]
async fn main() {
    let data_dir = resolve_data_dir();
    let gopeed = backend::resolve_gopeed_path(None);
    let mut options = backend::StartOptions::new(data_dir, storage_dir_override(), gopeed);
    options.poll_interval = Duration::from_secs(2);
    match backend::start(options).await {
        Ok(handle) => {
            println!("Rust backend listening on http://127.0.0.1:{}", handle.port);
            let _ = tokio::signal::ctrl_c().await;
            handle.state.gopeed.stop().await;
        }
        Err(err) => {
            eprintln!("Rust backend startup failed: {err}");
            std::process::exit(1);
        }
    }
}
