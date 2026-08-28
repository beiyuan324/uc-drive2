use std::path::PathBuf;
use std::time::Duration;

use serde_json::json;

use uc_drive2_lib::backend;
use uc_drive2_lib::backend::crypto::set_uc_cookie;
use uc_drive2_lib::backend::tasks::CreateTaskParams;
use uc_drive2_lib::backend::uc;

fn read_auth() -> Option<(String, String)> {
    let text =
        std::fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../ucAuth.txt"))
            .ok()?;
    let lines = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    let url = lines
        .iter()
        .position(|line| *line == "[url]")
        .and_then(|i| lines.get(i + 1))?;
    let cookie = lines
        .iter()
        .position(|line| *line == "[cookie]")
        .and_then(|i| lines.get(i + 1))?;
    Some(((*url).to_string(), (*cookie).to_string()))
}

#[tokio::test]
#[ignore = "真实 UC 网络与下载验证，显式 cargo test --test uc_e2e -- --ignored"]
async fn uc_真实链路_解析直链下载并登记() -> Result<(), String> {
    let Some((share_link, cookie)) = read_auth() else {
        return Ok(());
    };
    let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    let gopeed = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../bin/gopeed/gopeed-web.exe");
    if !gopeed.exists() {
        return Err(format!("未找到 gopeed-web.exe: {}", gopeed.display()));
    }
    let mut options = backend::StartOptions::new(dir.path().to_path_buf(), None, gopeed);
    options.poll_interval = Duration::from_secs(1);
    let state = backend::build_state(&options).map_err(|e| e.message)?;
    {
        let db = state.db.lock().unwrap();
        set_uc_cookie(&db, &state.paths.data_dir, &cookie);
    }
    state.tasks.spawn_loops();
    state.gopeed.start().await?;

    let parsed = uc::parse(&state.client, &share_link, &cookie).await?;
    if parsed.session.stoken.is_empty()
        || parsed.session.ctoken.is_empty()
        || parsed.files.is_empty()
    {
        state.gopeed.stop().await;
        return Err("UC 分享解析结果不完整".to_string());
    }
    let mut all = uc::find_files(
        &state.client,
        &parsed.share_id,
        &parsed.session.stoken,
        &parsed.session.ctoken,
        &parsed.session.cookies,
        parsed.pdir_fid.as_deref(),
    )
    .await?;
    all.sort_by_key(|file| file.size);
    let target = all
        .first()
        .ok_or_else(|| "分享中没有可下载文件".to_string())?
        .clone();
    let url = uc::get_download_url(
        &state.client,
        &parsed.share_id,
        &parsed.session.stoken,
        &target.fid,
        &target.share_fid_token,
        &parsed.session.ctoken,
        &parsed.session.cookies,
    )
    .await?;
    let probe = uc::probe_download_url(
        &state.client,
        &url,
        &parsed.session.cookies,
        Duration::from_secs(8),
    )
    .await;
    if !probe.ok {
        state.gopeed.stop().await;
        return Err(format!("UC 直链预检失败: {}", probe.kind));
    }

    let task = state
        .tasks
        .create(CreateTaskParams {
            source: "uc".to_string(),
            url: Some(url),
            torrent_id: None,
            torrent_name: None,
            filename: Some(target.name.clone()),
            headers: Some(json!({
                "Cookie": parsed.session.cookies,
                "User-Agent": uc::UA,
                "Referer": "https://drive.uc.cn/",
                "Origin": "https://drive.uc.cn",
                "x-csrf-token": parsed.session.ctoken,
            })),
            uc: Some(json!({
                "shareId": parsed.share_id,
                "shareLink": share_link,
                "fid": target.fid,
                "filename": target.name,
                "shareFidToken": target.share_fid_token,
                "size": target.size,
                "retryCount": 0,
                "lastRefreshAt": 0,
            })),
            connections: Some(300),
        })
        .await
        .map_err(|e| e.message)?;

    let deadline = tokio::time::Instant::now() + Duration::from_secs(180);
    let final_task = loop {
        let current = state.tasks.get(task.id).map_err(|e| e.message)?;
        if let Some(current) = current {
            if matches!(current.status.as_str(), "done" | "error" | "cookie_expired") {
                break current;
            }
        }
        if tokio::time::Instant::now() >= deadline {
            state.gopeed.stop().await;
            return Err("UC 任务 180 秒内未结束".to_string());
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    };
    state.gopeed.stop().await;
    if final_task.status != "done" {
        return Err(format!(
            "UC 任务未完成: {} {}",
            final_task.status, final_task.error
        ));
    }
    if final_task.progress != 100.0 {
        return Err(format!("UC 完成进度错误: {}", final_task.progress));
    }

    let db = state.db.lock().unwrap();
    let row: Option<(String, i64)> = db
        .query_row(
            "SELECT path, size FROM files WHERE path LIKE ?1 ORDER BY id DESC LIMIT 1",
            [format!(
                "{}/%",
                state.storage.get().to_string_lossy().replace('\\', "/")
            )],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .ok();
    let Some((path, size)) = row else {
        return Err("UC 完成文件未登记到文件树".to_string());
    };
    let disk_size = std::fs::metadata(&path).map_err(|e| e.to_string())?.len() as i64;
    if disk_size != size || (target.size > 0 && disk_size != target.size) {
        return Err(format!(
            "UC 登记大小不一致: share={} db={} disk={}",
            target.size, size, disk_size
        ));
    }
    Ok(())
}
