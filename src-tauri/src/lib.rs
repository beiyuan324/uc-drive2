pub mod backend;

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use backend::config::{resolve_data_dir, storage_dir_override};
use backend::winutil::terminate_pid;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{Manager, RunEvent, WindowEvent};

const MAIN_LABEL: &str = "main";

/// 后端端口文件：%APPDATA%/uc-drive2/server.port。
/// Rust 后端绑定端口后立即写入，前端仍沿用原有 invoke + 文件回退探测逻辑。
fn port_file_path() -> PathBuf {
    resolve_data_dir().join("server.port")
}

/// 读取后端实际监听端口。后端已在 Tauri 进程内启动，但保留文件轮询以兼容
/// 开发服务器、启动竞态和旧数据目录。
#[tauri::command]
async fn get_server_port() -> Result<u16, String> {
    let path = port_file_path();
    for _ in 0..100 {
        if let Ok(s) = fs::read_to_string(&path) {
            if let Ok(port) = s.trim().parse::<u16>() {
                return Ok(port);
            }
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Err("后端端口文件未就绪".into())
}

/// 在资源管理器中显示本地文件/目录：文件用 explorer /select 定位，目录直接打开。
#[tauri::command]
fn reveal_in_folder(path: String) -> Result<(), String> {
    let win_path = path.replace('/', "\\");
    let p = PathBuf::from(&win_path);
    let mut cmd = Command::new("explorer.exe");
    if p.is_dir() {
        cmd.arg(&win_path);
    } else {
        cmd.arg("/select,").arg(&win_path);
    }
    cmd.spawn().map_err(|e| e.to_string())?;
    Ok(())
}

// 强杀自身：避免 WebView2/CRT 清理阶段的退出卡顿。
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetCurrentProcess() -> *mut std::ffi::c_void;
    fn TerminateProcess(process: *mut std::ffi::c_void, exit_code: u32) -> i32;
}

fn force_exit(code: u32) -> ! {
    unsafe {
        TerminateProcess(GetCurrentProcess(), code);
    }
    std::process::exit(code as i32)
}

/// Tauri 退出时需要清理的 gopeed pid。Rust 后端与 UI 是同一进程。
struct BackendChild {
    gopeed_pid: Arc<Mutex<Option<u32>>>,
}

fn kill_backend(pid_sink: &Arc<Mutex<Option<u32>>>) {
    if let Some(pid) = *pid_sink.lock().unwrap() {
        terminate_pid(pid);
        // gopeed 可能启动辅助子进程；异步 taskkill 兜底，不阻塞托盘退出。
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            let _ = Command::new("taskkill")
                .args(["/PID", &pid.to_string(), "/T", "/F"])
                .creation_flags(0x0800_0000)
                .spawn();
        }
    }
}

fn setup_tray(app: &tauri::AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let show = MenuItem::with_id(app, "show", "显示窗口", true, None::<&str>)?;
    let downloads = MenuItem::with_id(app, "downloads", "下载管理", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &downloads, &quit])?;

    TrayIconBuilder::with_id("main-tray")
        .icon(
            app.default_window_icon()
                .cloned()
                .ok_or("no default icon")?,
        )
        .tooltip("uc-drive2 桌面网盘")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_main_window(app),
            "downloads" => {
                show_main_window(app);
                if let Some(window) = app.get_webview_window(MAIN_LABEL) {
                    let _ = window.eval("location.hash = '#/downloads'");
                }
            }
            "quit" => {
                if let Some(child) = app.try_state::<BackendChild>() {
                    kill_backend(&child.gopeed_pid);
                }
                force_exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

fn show_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window(MAIN_LABEL) {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

/// 去掉 Windows 资源路径的 \\?\ 前缀。Command 通常能处理长路径，但普通路径更兼容。
fn strip_verbatim(path: &std::path::Path) -> PathBuf {
    let text = path.to_string_lossy();
    PathBuf::from(text.strip_prefix("\\\\?\\").unwrap_or(&text))
}

fn start_backend(app: &tauri::AppHandle, pid_sink: Arc<Mutex<Option<u32>>>) {
    let resource_dir = app.path().resource_dir().ok();
    let gopeed_exe = strip_verbatim(&backend::resolve_gopeed_path(resource_dir.as_deref()));
    let data_dir = resolve_data_dir();
    let storage_dir = storage_dir_override();
    let mut options = backend::StartOptions::new(data_dir, storage_dir, gopeed_exe);
    options.pid_sink = Some(pid_sink);

    tauri::async_runtime::spawn(async move {
        match backend::start(options).await {
            Ok(handle) => {
                if let Err(err) = handle.wait().await {
                    eprintln!("[tauri] Rust 后端已停止: {err}");
                }
            }
            Err(err) => eprintln!("[tauri] Rust 后端启动失败: {err}"),
        }
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let pid_sink = Arc::new(Mutex::new(None));
            app.manage(BackendChild {
                gopeed_pid: pid_sink.clone(),
            });
            start_backend(app.handle(), pid_sink);
            if let Err(err) = setup_tray(app.handle()) {
                eprintln!("[tauri] 托盘创建失败: {err}");
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![get_server_port, reveal_in_folder])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app_handle, event| match event {
            // 关闭窗口 → 最小化到托盘（后台运行）；托盘「退出」才真正退出。
            RunEvent::WindowEvent {
                label,
                event: WindowEvent::CloseRequested { api, .. },
                ..
            } if label == MAIN_LABEL => {
                api.prevent_close();
                if let Some(window) = app_handle.get_webview_window(MAIN_LABEL) {
                    let _ = window.hide();
                }
            }
            RunEvent::Exit => {
                if let Some(child) = app_handle.try_state::<BackendChild>() {
                    kill_backend(&child.gopeed_pid);
                }
            }
            _ => {}
        });
}
