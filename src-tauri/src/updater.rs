//! 更新对象和经过签名验证的安装包只保留在 Rust 内存中。
use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_updater::{Update, UpdaterExt};
use tokio::sync::Mutex;

use crate::backend::AppState;

#[derive(Default)]
pub struct AppUpdater {
    pending: Mutex<Option<PendingUpdate>>,
}

struct PendingUpdate {
    update: Update,
    bytes: Option<Vec<u8>>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    version: String,
    notes: String,
    published_at: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckResult {
    current_version: String,
    update: Option<UpdateInfo>,
}

#[derive(Clone, Serialize)]
#[serde(
    tag = "event",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum UpdateEvent {
    Progress { downloaded: u64, total: Option<u64> },
    Downloaded,
    Error { message: String },
}

#[derive(Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum InstallResult {
    Blocked { reason: String },
    Installed,
}

fn install_supported() -> Result<(), String> {
    if cfg!(debug_assertions) || !cfg!(all(target_os = "windows", target_arch = "x86_64")) {
        return Err("仅 Windows x64 正式安装版支持下载和安装更新".into());
    }
    Ok(())
}

#[tauri::command]
pub async fn check_app_update(
    app: AppHandle,
    state: State<'_, AppUpdater>,
) -> Result<CheckResult, String> {
    let mut pending = state
        .pending
        .try_lock()
        .map_err(|_| "更新操作正在进行，请稍后再试")?;
    // 已下载的更新保持不变，避免定时检查丢弃安装包。
    if let Some(cached) = pending.as_ref().filter(|p| p.bytes.is_some()) {
        return Ok(CheckResult {
            current_version: cached.update.current_version.clone(),
            update: Some(info(&cached.update)),
        });
    }
    let cleanup_app = app.clone();
    let updater = app
        .updater_builder()
        .timeout(Duration::from_secs(15 * 60))
        .restart_after_install(true)
        .version_comparator(|current, remote| {
            remote.version > current && remote.version.pre.is_empty()
        })
        .on_before_exit(move || {
            if let Some(backend) = cleanup_app.try_state::<Arc<AppState>>() {
                backend.gopeed.before_update_exit();
            }
        })
        .build()
        .map_err(|e| format!("更新配置不可用：{e}"))?;
    let update = tokio::time::timeout(Duration::from_secs(30), updater.check())
        .await
        .map_err(|_| "检查更新超时，请检查网络后重试".to_string())?
        .map_err(|e| format!("检查更新失败：{e}"))?;
    if let Some(update) = update.as_ref() {
        if update.download_url.scheme() != "https" {
            return Err("更新下载地址必须使用 HTTPS".into());
        }
    }
    let result = CheckResult {
        current_version: app.package_info().version.to_string(),
        update: update.as_ref().map(info),
    };
    *pending = update.map(|update| PendingUpdate {
        update,
        bytes: None,
    });
    Ok(result)
}

fn info(update: &Update) -> UpdateInfo {
    UpdateInfo {
        version: update.version.clone(),
        notes: update.body.clone().unwrap_or_default(),
        published_at: update.date.map(|date| date.to_string()),
    }
}

#[tauri::command]
pub async fn download_app_update(
    state: State<'_, AppUpdater>,
    on_event: Channel<UpdateEvent>,
) -> Result<(), String> {
    install_supported()?;
    let mut pending = state
        .pending
        .try_lock()
        .map_err(|_| "更新操作正在进行，请稍后再试")?;
    let cached = pending.as_mut().ok_or("请先检查更新")?;
    if cached.bytes.is_some() {
        let _ = on_event.send(UpdateEvent::Downloaded);
        return Ok(());
    }
    let mut downloaded = 0u64;
    let bytes = cached
        .update
        .download(
            |chunk, total| {
                downloaded += chunk as u64;
                let _ = on_event.send(UpdateEvent::Progress { downloaded, total });
            },
            || {},
        )
        .await
        .map_err(|e| {
            let message = format!("更新下载或签名校验失败：{e}");
            let _ = on_event.send(UpdateEvent::Error {
                message: message.clone(),
            });
            message
        })?;
    cached.bytes = Some(bytes);
    let _ = on_event.send(UpdateEvent::Downloaded);
    Ok(())
}

#[tauri::command]
pub async fn install_app_update(
    app: AppHandle,
    state: State<'_, AppUpdater>,
) -> Result<InstallResult, String> {
    install_supported()?;
    let mut pending = state
        .pending
        .try_lock()
        .map_err(|_| "更新操作正在进行，请稍后再试")?;
    let cached = pending.as_mut().ok_or("请先检查并下载更新")?;
    let bytes = cached
        .bytes
        .as_ref()
        .ok_or("更新尚未下载或签名校验未通过")?;
    let backend = app
        .try_state::<Arc<AppState>>()
        .ok_or("后端尚未就绪，请稍后重试")?
        .inner()
        .clone();
    let gate = match backend.update_gate.begin_install() {
        Ok(gate) => gate,
        Err(reason) => return Ok(InstallResult::Blocked { reason }),
    };
    let active: i64 = backend
        .db
        .lock()
        .map_err(|_| "数据库繁忙")?
        .query_row(
            "SELECT COUNT(*) FROM tasks WHERE status IN ('queued', 'running')",
            [],
            |r| r.get(0),
        )
        .map_err(|e| format!("无法确认任务状态：{e}"))?;
    if active > 0 {
        return Ok(InstallResult::Blocked {
            reason: format!("仍有 {active} 个排队或运行中的下载任务，请完成或暂停后安装"),
        });
    }
    if let Err(error) = backend.gopeed.stop_for_update().await {
        drop(gate);
        let recovery = backend.gopeed.start().await;
        return Err(recovery
            .err()
            .map_or(error.clone(), |e| format!("{error}；下载引擎恢复失败：{e}")));
    }
    // Windows 插件成功启动 NSIS 后会直接退出进程。同步安装在阻塞线程执行，
    // UAC 取消或 ShellExecute 失败则恢复引擎，保留安装包供重试。
    let update = cached.update.clone();
    let bytes = bytes.clone();
    let installed = tauri::async_runtime::spawn_blocking(move || update.install(bytes))
        .await
        .map_err(|e| format!("安装线程失败：{e}"))
        .and_then(|r| r.map_err(|e| format!("启动安装程序失败：{e}")));
    if let Err(error) = installed {
        drop(gate);
        let recovery = backend.gopeed.start().await;
        return Err(recovery
            .err()
            .map_or(error.clone(), |e| format!("{error}；下载引擎恢复失败：{e}")));
    }
    Ok(InstallResult::Installed)
}
