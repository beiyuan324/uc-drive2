use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use tauri::{Manager, RunEvent, WindowEvent};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri_plugin_shell::ShellExt;

const MAIN_LABEL: &str = "main";

/// 后端端口文件：%APPDATA%/uc-drive2/server.port
fn port_file_path() -> PathBuf {
    let appdata = std::env::var("APPDATA").unwrap_or_else(|_| ".".into());
    PathBuf::from(appdata).join("uc-drive2").join("server.port")
}

/// 读取后端实际监听端口（node 启动后异步写入，这里轮询等待）
#[tauri::command]
fn get_server_port() -> Result<u16, String> {
    let path = port_file_path();
    for _ in 0..100 {
        if let Ok(s) = fs::read_to_string(&path) {
            if let Ok(port) = s.trim().parse::<u16>() {
                return Ok(port);
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Err("后端端口文件未就绪".into())
}

/// 在资源管理器中显示本地文件/目录：文件用 explorer /select, 定位，目录直接打开
#[tauri::command]
fn reveal_in_folder(path: String) -> Result<(), String> {
    // explorer 的命令行解析对正斜杠/引号位置敏感：正斜杠或 /select,<path> 单参数带空格时
    // 会解析失败并回退打开「文档」目录（实测验证）。必须：① 转反斜杠；② /select, 与路径分开传参。
    let win_path = path.replace('/', "\\");
    let p = PathBuf::from(&win_path);
    let mut cmd = std::process::Command::new("explorer.exe");
    if p.is_dir() {
        cmd.arg(&win_path);
    } else {
        cmd.arg("/select,").arg(&win_path);
    }
    cmd.spawn().map_err(|e| e.to_string())?;
    Ok(())
}

// 强杀自身：std::process::exit(0) 仍会走 CRT 的 atexit / 静态析构 / DLL detach
// （msedgewebview2.dll 等卸载时实测仍可卡数秒）。TerminateProcess 跳过一切清理，
// 进程立即消失，无窗体闪烁；后端已在调用前杀掉，不会产生孤儿。
#[link(name = "kernel32")]
unsafe extern "system" {
    fn OpenProcess(desired_access: u32, inherit_handle: i32, process_id: u32) -> *mut std::ffi::c_void;
    fn GetCurrentProcess() -> *mut std::ffi::c_void;
    fn TerminateProcess(process: *mut std::ffi::c_void, exit_code: u32) -> i32;
    fn CloseHandle(object: *mut std::ffi::c_void) -> i32;
}
const PROCESS_TERMINATE: u32 = 0x0001;

fn force_exit(code: u32) -> ! {
    unsafe {
        TerminateProcess(GetCurrentProcess(), code);
    }
    // TerminateProcess 成功后不会返回；极端失败时兜底退出
    std::process::exit(code as i32)
}

/// 直接强杀指定 pid（尽力而为：进程不存在/权限不足时静默失败）
fn terminate_pid(pid: u32) {
    unsafe {
        let h = OpenProcess(PROCESS_TERMINATE, 0, pid);
        if !h.is_null() {
            TerminateProcess(h, 1);
            CloseHandle(h);
        }
    }
}

fn backend_state_path() -> PathBuf {
    let appdata = std::env::var("APPDATA").unwrap_or_else(|_| ".".into());
    PathBuf::from(appdata).join("uc-drive2").join("backend-state.json")
}

/// 杀后端：直接 TerminateProcess node（sidecar pid）+ 读后端状态文件杀 gopeed，
/// 零等待零外部依赖；另 spawn 一个不等待的 taskkill /T 兜底清理可能的孙进程。
/// （历史坑：同步 taskkill .status() 实测会等 5 秒+，绝不可用。）
fn kill_backend(pid: u32) {
    // 1) 直接强杀 node
    terminate_pid(pid);
    // 2) 读后端写的状态文件，强杀 gopeed
    if let Ok(s) = std::fs::read_to_string(backend_state_path()) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&s) {
            if let Some(g) = v.get("gopeed").and_then(|x| x.as_u64()) {
                terminate_pid(g as u32);
            }
        }
    }
    // 3) 兜底：不等待的 taskkill，清理树中可能存在的其他孙进程
    use std::os::windows::process::CommandExt;
    let _ = Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
        .spawn();
}

struct BackendChild {
    pid: u32,
}

/// tauri 的 resource_dir() 在 Windows 返回带 `\\?\` 长路径前缀的路径，
/// node 无法解析该前缀（会报 EISDIR: lstat 'C:'），这里剥离掉。
fn strip_verbatim(p: &std::path::Path) -> PathBuf {
    let s = p.to_string_lossy();
    let s = s.strip_prefix(r"\\?\").unwrap_or(&s);
    PathBuf::from(s.to_string())
}

fn setup_tray(app: &tauri::AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let show = MenuItem::with_id(app, "show", "显示窗口", true, None::<&str>)?;
    let downloads = MenuItem::with_id(app, "downloads", "下载管理", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &downloads, &quit])?;

    TrayIconBuilder::with_id("main-tray")
        .icon(app.default_window_icon().cloned().ok_or("no default icon")?)
        .tooltip("uc-drive2 桌面网盘")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => {
                show_main_window(app);
            }
            "downloads" => {
                show_main_window(app);
                if let Some(w) = app.get_webview_window(MAIN_LABEL) {
                    let _ = w.eval("location.hash = '#/downloads'");
                }
            }
            "quit" => {
                // 先杀后端（直接 TerminateProcess node + gopeed，实测 ~4ms），再强杀自身，
                // 彻底规避 WebView2/CRT 清理卡顿与窗体闪烁。
                if let Some(child) = app.try_state::<BackendChild>() {
                    kill_backend(child.pid);
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
    if let Some(w) = app.get_webview_window(MAIN_LABEL) {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

fn spawn_backend(app: &tauri::AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let resource_dir = app.path().resource_dir()?;

    // tauri 2.5+ 把 resources 放到 exe 旁的 _up_ 目录（旧版本平铺在 resource_dir 下，
    // updater 布局还有 _up_/<hash> 子目录），这里依次探测兼容全部布局。
    let find = |rel: &str| -> Option<PathBuf> {
        let mut candidates = vec![
            resource_dir.join("_up_").join(rel),
            resource_dir.join(rel),
        ];
        if let Ok(rd) = std::fs::read_dir(resource_dir.join("_up_")) {
            for e in rd.flatten() {
                if e.path().is_dir() {
                    let p = e.path().join(rel);
                    if p.exists() {
                        candidates.push(p);
                    }
                }
            }
        }
        candidates.into_iter().find(|p| p.exists())
    };

    let server_js = find("server.js").ok_or("找不到后端 server.js 资源")?;
    let gopeed = find("gopeed-web.exe").ok_or("找不到 gopeed-web.exe 资源")?;
    let server_js = strip_verbatim(&server_js);
    let gopeed = strip_verbatim(&gopeed);
    eprintln!("[tauri] server.js = {}", server_js.display());
    eprintln!("[tauri] gopeed = {}", gopeed.display());

    let (mut rx, child) = app
        .shell()
        .sidecar("node")?
        .args([server_js.to_string_lossy().to_string()])
        .env("GOPEED_PATH", gopeed.to_string_lossy().to_string())
        .spawn()?;

    app.manage(BackendChild { pid: child.pid() });

    tauri::async_runtime::spawn(async move {
        use tauri_plugin_shell::process::CommandEvent;
        while let Some(event) = rx.recv().await {
            match event {
                CommandEvent::Stdout(line) => {
                    print!("[node] {}", String::from_utf8_lossy(&line));
                }
                CommandEvent::Stderr(line) => {
                    eprint!("[node:err] {}", String::from_utf8_lossy(&line));
                }
                _ => {}
            }
        }
    });

    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            if let Err(err) = spawn_backend(app.handle()) {
                eprintln!("[tauri] 后端启动失败: {err}");
            }
            if let Err(err) = setup_tray(app.handle()) {
                eprintln!("[tauri] 托盘创建失败: {err}");
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![get_server_port, reveal_in_folder])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app_handle, event| {
            match event {
                // 关闭窗口 → 最小化到托盘（后台运行）；托盘「退出」才真正退出
                RunEvent::WindowEvent {
                    label,
                    event: WindowEvent::CloseRequested { api, .. },
                    ..
                } if label == MAIN_LABEL => {
                    api.prevent_close();
                    if let Some(w) = app_handle.get_webview_window(MAIN_LABEL) {
                        let _ = w.hide();
                    }
                }
                RunEvent::Exit => {
                    if let Some(child) = app_handle.try_state::<BackendChild>() {
                        kill_backend(child.pid);
                    }
                }
                _ => {}
            }
        });
}
