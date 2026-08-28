//! 下载任务服务：任务表为本地记录，gopeed 负责实际下载。
//! 保留现有状态映射、进度算法、UC 直链刷新和完成后登记语义。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};

use super::config::Storage;
use super::crypto::{get_uc_cookie, set_setting};
use super::error::{ApiError, ApiResult};
use super::files;
use super::gopeed::{GopeedEvent, GopeedManager};
use super::models::{DownloadConfigDto, TaskDto};
use super::uc;
use super::util::{basename_of, norm_path, now_iso, now_ms, relative_suffix, unique_path};

#[derive(Debug, Clone)]
struct TaskRow {
    id: i64,
    gopeed_id: String,
    source: String,
    source_url: String,
    status: String,
    progress: f64,
    speed: i64,
    error: String,
    target_dir: String,
    metadata: String,
    created_at: String,
    updated_at: String,
    finished_at: Option<String>,
}

impl TaskRow {
    fn dto(&self) -> TaskDto {
        let metadata = serde_json::from_str(&self.metadata).unwrap_or_else(|_| json!({}));
        TaskDto {
            id: self.id,
            gopeed_id: self.gopeed_id.clone(),
            source: self.source.clone(),
            source_url: self.source_url.clone(),
            status: self.status.clone(),
            progress: self.progress,
            speed: self.speed,
            error: self.error.clone(),
            target_dir: self.target_dir.clone(),
            metadata,
            created_at: self.created_at.clone(),
            updated_at: self.updated_at.clone(),
            finished_at: self.finished_at.clone(),
        }
    }

    fn meta(&self) -> Value {
        serde_json::from_str(&self.metadata).unwrap_or_else(|_| json!({}))
    }
}

fn task_from_sql(row: &rusqlite::Row<'_>) -> rusqlite::Result<TaskRow> {
    Ok(TaskRow {
        id: row.get(0)?,
        gopeed_id: row.get(1)?,
        source: row.get(2)?,
        source_url: row.get(3)?,
        status: row.get(4)?,
        progress: row.get(5)?,
        speed: row.get(6)?,
        error: row.get(7)?,
        target_dir: row.get(8)?,
        metadata: row.get(9)?,
        created_at: row.get(10)?,
        updated_at: row.get(11)?,
        finished_at: row.get(12)?,
    })
}

fn read_row(db: &Connection, id: i64) -> ApiResult<Option<TaskRow>> {
    db.query_row(
        "SELECT id, gopeed_id, source, source_url, status, progress, speed, error,
                target_dir, metadata, created_at, updated_at, finished_at
         FROM tasks WHERE id = ?1",
        [id],
        task_from_sql,
    )
    .optional()
    .map_err(|e| ApiError::internal(format!("数据库错误: {e}")))
}

fn rows(db: &Connection, sql: &str) -> ApiResult<Vec<TaskRow>> {
    let mut stmt = db
        .prepare(sql)
        .map_err(|e| ApiError::internal(format!("数据库错误: {e}")))?;
    let mapped = stmt
        .query_map([], task_from_sql)
        .map_err(|e| ApiError::internal(format!("数据库错误: {e}")))?;
    mapped
        .map(|r| r.map_err(|e| ApiError::internal(format!("数据库错误: {e}"))))
        .collect()
}

#[derive(Debug, Clone)]
pub struct CreateTaskParams {
    pub source: String,
    pub url: Option<String>,
    pub torrent_id: Option<i64>,
    pub torrent_name: Option<String>,
    pub filename: Option<String>,
    pub headers: Option<Value>,
    pub uc: Option<Value>,
    pub connections: Option<i64>,
}

#[derive(Debug, Clone, Default)]
pub struct ConfigPatch {
    pub uc_connections: Option<Value>,
    pub http_connections: Option<Value>,
    pub max_running: Option<Value>,
}

#[derive(Debug, Clone, Copy)]
struct SpeedPoint {
    bytes: u64,
    at: Instant,
}

pub struct TaskService {
    pub db: Arc<Mutex<Connection>>,
    pub storage: Storage,
    pub data_dir: PathBuf,
    pub gopeed: Arc<GopeedManager>,
    pub client: reqwest::Client,
    speed_cache: Mutex<HashMap<i64, SpeedPoint>>,
    poll_interval: Duration,
}

impl std::fmt::Debug for TaskService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TaskService")
            .field("storage", &self.storage.get())
            .field("poll_interval", &self.poll_interval)
            .finish()
    }
}

impl TaskService {
    pub fn new(
        db: Arc<Mutex<Connection>>,
        storage: Storage,
        data_dir: PathBuf,
        gopeed: Arc<GopeedManager>,
        client: reqwest::Client,
        poll_interval: Duration,
    ) -> Arc<Self> {
        Arc::new(TaskService {
            db,
            storage,
            data_dir,
            gopeed,
            client,
            speed_cache: Mutex::new(HashMap::new()),
            poll_interval,
        })
    }

    /// 启动 gopeed 重启事件监听和 2 秒轮询。无 queued/running 任务时不请求 gopeed。
    pub fn spawn_loops(self: &Arc<Self>) {
        let event_service = Arc::clone(self);
        let mut events = self.gopeed.subscribe();
        tokio::spawn(async move {
            loop {
                match events.recv().await {
                    Ok(GopeedEvent::Started) => event_service.on_started().await,
                    Ok(GopeedEvent::Exited { .. }) => {}
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        });

        let poll_service = Arc::clone(self);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(poll_service.poll_interval);
            // 第一次回调在一个完整间隔之后。
            interval.tick().await;
            loop {
                interval.tick().await;
                if !poll_service.has_sync_work() || !poll_service.gopeed.ready() {
                    continue;
                }
                if let Ok(list) = poll_service.gopeed.list_tasks().await {
                    let _ = poll_service.sync_from_gopeed(list).await;
                }
            }
        });
    }

    async fn on_started(&self) {
        self.resume_interrupted().await;
        let _ = self.apply_config_to_gopeed().await;
    }

    /// 读取下载参数（存 settings，重启后保留）。
    pub fn get_config(&self) -> DownloadConfigDto {
        let db = self.db.lock().unwrap();
        DownloadConfigDto {
            uc_connections: self.get_setting_locked(&db, "uc_connections", 300),
            http_connections: self.get_setting_locked(&db, "http_connections", 0),
            max_running: self.get_setting_locked(&db, "max_running", 3),
        }
    }

    fn get_setting_locked(&self, db: &Connection, key: &str, default: i64) -> i64 {
        db.query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| {
            r.get::<_, String>(0)
        })
        .ok()
        .and_then(|v| v.parse::<f64>().ok())
        .filter(|v| v.is_finite())
        .map(|v| v as i64)
        .unwrap_or(default)
    }

    pub async fn set_config(&self, patch: ConfigPatch) -> ApiResult<DownloadConfigDto> {
        {
            let db = self.db.lock().unwrap();
            if let Some(v) = patch.uc_connections {
                set_setting(
                    &db,
                    "uc_connections",
                    &clamp_number(v, 1, 1000)?.to_string(),
                );
            }
            if let Some(v) = patch.http_connections {
                set_setting(
                    &db,
                    "http_connections",
                    &clamp_number(v, 0, 1000)?.to_string(),
                );
            }
            if let Some(v) = patch.max_running {
                set_setting(&db, "max_running", &clamp_number(v, 1, 10)?.to_string());
            }
        }
        let _ = self.apply_config_to_gopeed().await;
        Ok(self.get_config())
    }

    async fn apply_config_to_gopeed(&self) -> Result<(), String> {
        if !self.gopeed.ready() {
            return Ok(());
        }
        let cfg = self.gopeed.get_config().await?;
        let max_running = self.get_config().max_running;
        let current = cfg.get("maxRunning").and_then(Value::as_i64).unwrap_or(0);
        if current != max_running {
            let mut next = cfg;
            if let Some(obj) = next.as_object_mut() {
                obj.insert("maxRunning".to_string(), json!(max_running));
            }
            self.gopeed.put_config(next).await?;
        }
        Ok(())
    }

    async fn resume_interrupted(&self) {
        let pending = {
            let db = self.db.lock().unwrap();
            rows(
                &db,
                "SELECT id, gopeed_id, source, source_url, status, progress, speed, error,
                        target_dir, metadata, created_at, updated_at, finished_at
                 FROM tasks WHERE status IN ('running','queued') AND gopeed_id != ''",
            )
            .unwrap_or_default()
        };
        for row in pending {
            if let Ok(task) = self.gopeed.get_task(&row.gopeed_id).await {
                let status = task.get("status").and_then(Value::as_str).unwrap_or("");
                if matches!(status, "ready" | "pause" | "wait") {
                    let _ = self.gopeed.resume(&row.gopeed_id).await;
                }
            }
        }
    }

    pub fn has_sync_work(&self) -> bool {
        let db = self.db.lock().unwrap();
        db.query_row(
            "SELECT 1 FROM tasks WHERE gopeed_id != '' AND status IN ('queued','running') LIMIT 1",
            [],
            |_r| Ok(1i64),
        )
        .is_ok()
    }

    pub fn list(&self) -> ApiResult<Vec<TaskDto>> {
        let db = self.db.lock().unwrap();
        Ok(rows(
            &db,
            "SELECT id, gopeed_id, source, source_url, status, progress, speed, error,
                    target_dir, metadata, created_at, updated_at, finished_at
             FROM tasks ORDER BY id DESC",
        )?
        .iter()
        .map(TaskRow::dto)
        .collect())
    }

    pub fn get(&self, id: i64) -> ApiResult<Option<TaskDto>> {
        let db = self.db.lock().unwrap();
        Ok(read_row(&db, id)?.map(|r| r.dto()))
    }

    fn get_row(&self, id: i64) -> ApiResult<Option<TaskRow>> {
        let db = self.db.lock().unwrap();
        read_row(&db, id)
    }

    pub fn history(&self) -> ApiResult<Vec<TaskDto>> {
        let db = self.db.lock().unwrap();
        Ok(rows(
            &db,
            "SELECT id, gopeed_id, source, source_url, status, progress, speed, error,
                    target_dir, metadata, created_at, updated_at, finished_at
             FROM tasks WHERE status IN ('done','error','cookie_expired','replaced')
             ORDER BY updated_at DESC LIMIT 300",
        )?
        .iter()
        .map(TaskRow::dto)
        .collect())
    }

    pub fn clear_history(&self) -> ApiResult<i64> {
        let db = self.db.lock().unwrap();
        db.execute(
            "DELETE FROM tasks WHERE status IN ('done','error','cookie_expired','replaced')",
            [],
        )
        .map(|n| n as i64)
        .map_err(|e| ApiError::internal(format!("数据库错误: {e}")))
    }

    /// 创建任务：source=url|magnet|torrent|uc。
    pub async fn create(&self, params: CreateTaskParams) -> ApiResult<TaskDto> {
        let offline = self.storage.offline_dir();
        std::fs::create_dir_all(&offline).map_err(|e| ApiError::from_io(&e))?;
        let mut url = params.url.clone().unwrap_or_default();
        let mut metadata = json!({});
        let mut temporary_torrent: Option<PathBuf> = None;

        if params.source == "uc" {
            metadata = json!({ "uc": params.uc.clone().unwrap_or_else(|| json!({})) });
        } else if params.source == "torrent" {
            if let Some(name) = params.torrent_name.as_deref() {
                let candidate = self
                    .data_dir
                    .join("tmp")
                    .join("torrents")
                    .join(basename_of(name));
                if !candidate
                    .to_string_lossy()
                    .to_lowercase()
                    .ends_with(".torrent")
                    || !candidate.exists()
                {
                    return Err(ApiError::internal("无效的 torrent 文件"));
                }
                url = format!("file:///{}", candidate.to_string_lossy().replace('\\', "/"));
                temporary_torrent = Some(candidate);
            } else if let Some(id) = params.torrent_id {
                let valid = {
                    let db = self.db.lock().unwrap();
                    files::get_row(&db, id).filter(|r| r.path.to_lowercase().ends_with(".torrent"))
                };
                let Some(row) = valid else {
                    return Err(ApiError::internal("无效的 torrent 文件"));
                };
                url = format!("file:///{}", row.path.replace('\\', "/"));
            }
        }

        let lower = url.to_lowercase();
        if url.is_empty()
            || !(lower.starts_with("http://")
                || lower.starts_with("https://")
                || lower.starts_with("magnet:")
                || lower.starts_with("file://"))
        {
            if let Some(path) = temporary_torrent {
                let _ = std::fs::remove_file(path);
            }
            return Err(ApiError::internal("链接不合法"));
        }

        let random = rand::Rng::gen_range(&mut rand::thread_rng(), 0..1000u32);
        let temp_dir = offline.join(format!("task-{}-{}", now_ms(), random));
        std::fs::create_dir_all(&temp_dir).map_err(|e| ApiError::from_io(&e))?;
        let (id, final_dir) = {
            let db = self.db.lock().unwrap();
            let now = now_iso();
            db.execute(
                "INSERT INTO tasks (source, source_url, status, target_dir, metadata, created_at, updated_at)
                 VALUES (?1, ?2, 'queued', ?3, ?4, ?5, ?6)",
                params![
                    params.source,
                    url,
                    norm_path(&temp_dir),
                    serde_json::to_string(&metadata).unwrap_or_else(|_| "{}".to_string()),
                    now,
                    now,
                ],
            )
            .map_err(|e| ApiError::internal(format!("数据库错误: {e}")))?;
            let id = db.last_insert_rowid();
            let final_dir = offline.join(format!("task-{id}"));
            if let Err(e) = std::fs::rename(&temp_dir, &final_dir) {
                let _ = db.execute("DELETE FROM tasks WHERE id = ?1", [id]);
                return Err(ApiError::from_io(&e));
            }
            db.execute(
                "UPDATE tasks SET target_dir = ?1 WHERE id = ?2",
                params![norm_path(&final_dir), id],
            )
            .map_err(|e| ApiError::internal(format!("数据库错误: {e}")))?;
            (id, final_dir)
        };

        let result = self
            .create_gopeed_task(&params, &url, &final_dir, metadata.clone())
            .await;
        let result = match result {
            Ok(gid) => {
                {
                    let db = self.db.lock().unwrap();
                    db.execute(
                        "UPDATE tasks SET gopeed_id = ?1, status = 'queued', updated_at = ?2 WHERE id = ?3",
                        params![gid, now_iso(), id],
                    )
                    .map_err(|e| ApiError::internal(format!("数据库错误: {e}")))?;
                }
                self.get(id)?
                    .ok_or_else(|| ApiError::internal("任务登记失败"))
            }
            Err(err) => {
                let _ = std::fs::remove_dir_all(&final_dir);
                let db = self.db.lock().unwrap();
                let message = err.message.clone();
                let _ = db.execute(
                    "UPDATE tasks SET status = 'error', error = ?1, updated_at = ?2 WHERE id = ?3",
                    params![message, now_iso(), id],
                );
                Err(err)
            }
        };
        if let Some(path) = temporary_torrent {
            let _ = std::fs::remove_file(path);
        }
        result
    }

    async fn create_gopeed_task(
        &self,
        params: &CreateTaskParams,
        url: &str,
        final_dir: &Path,
        _metadata: Value,
    ) -> ApiResult<String> {
        let mut request = json!({ "url": url });
        if let Some(headers) = params
            .headers
            .as_ref()
            .filter(|h| h.as_object().map(|o| !o.is_empty()).unwrap_or(false))
        {
            request["extra"] = json!({ "header": headers });
        }
        let mut opts = json!({ "path": final_dir.to_string_lossy().to_string() });
        if let Some(filename) = params.filename.as_ref().filter(|s| !s.is_empty()) {
            opts["name"] = json!(filename);
        }
        let connections = match params.connections {
            Some(v) => v,
            None if params.source == "uc" => self.get_config().uc_connections,
            None if params.source == "url" => self.get_config().http_connections,
            None => 0,
        };
        if connections > 0 && matches!(params.source.as_str(), "uc" | "url") {
            opts["extra"] = json!({ "connections": connections });
        }
        self.gopeed
            .create_task(request, opts)
            .await
            .map_err(ApiError::internal)
    }

    pub async fn pause(&self, id: i64) -> ApiResult<TaskDto> {
        let row = self
            .get_row(id)?
            .ok_or_else(|| ApiError::internal("任务不存在"))?;
        if row.gopeed_id.is_empty() {
            return Err(ApiError::internal("任务不存在"));
        }
        self.gopeed
            .pause(&row.gopeed_id)
            .await
            .map_err(ApiError::internal)?;
        {
            let db = self.db.lock().unwrap();
            db.execute(
                "UPDATE tasks SET status = 'paused', updated_at = ?1 WHERE id = ?2",
                params![now_iso(), id],
            )
            .map_err(|e| ApiError::internal(format!("数据库错误: {e}")))?;
        }
        self.get(id)?
            .ok_or_else(|| ApiError::internal("任务不存在"))
    }

    pub async fn resume(&self, id: i64) -> ApiResult<TaskDto> {
        let row = self
            .get_row(id)?
            .ok_or_else(|| ApiError::internal("任务不存在"))?;
        if row.gopeed_id.is_empty() {
            return Err(ApiError::internal("任务不存在"));
        }
        self.gopeed
            .resume(&row.gopeed_id)
            .await
            .map_err(ApiError::internal)?;
        {
            let db = self.db.lock().unwrap();
            db.execute(
                "UPDATE tasks SET status = 'running', updated_at = ?1 WHERE id = ?2",
                params![now_iso(), id],
            )
            .map_err(|e| ApiError::internal(format!("数据库错误: {e}")))?;
        }
        self.get(id)?
            .ok_or_else(|| ApiError::internal("任务不存在"))
    }

    pub async fn remove(&self, id: i64, force: bool) -> ApiResult<()> {
        let row = self
            .get_row(id)?
            .ok_or_else(|| ApiError::internal("任务不存在"))?;
        if !row.gopeed_id.is_empty() {
            let _ = self.gopeed.remove(&row.gopeed_id, force).await;
        }
        if force {
            let _ = std::fs::remove_dir_all(&row.target_dir);
            let db = self.db.lock().unwrap();
            let like = format!("{}/*", row.target_dir);
            db.execute(
                "DELETE FROM files WHERE path = ?1 OR path LIKE ?2",
                params![row.target_dir, like],
            )
            .map_err(|e| ApiError::internal(format!("数据库错误: {e}")))?;
        }
        let db = self.db.lock().unwrap();
        db.execute("DELETE FROM tasks WHERE id = ?1", [id])
            .map_err(|e| ApiError::internal(format!("数据库错误: {e}")))?;
        Ok(())
    }

    /// 同步一轮 gopeed 任务结果。
    pub async fn sync_from_gopeed(&self, gopeed_tasks: Vec<Value>) -> ApiResult<()> {
        let by_id: HashMap<String, Value> = gopeed_tasks
            .into_iter()
            .filter_map(|task| value_string(task.get("id")).map(|id| (id, task)))
            .collect();
        let local_rows = {
            let db = self.db.lock().unwrap();
            rows(
                &db,
                "SELECT id, gopeed_id, source, source_url, status, progress, speed, error,
                        target_dir, metadata, created_at, updated_at, finished_at
                 FROM tasks WHERE gopeed_id != ''",
            )?
        };
        for row in local_rows {
            let Some(g) = by_id.get(&row.gopeed_id) else {
                continue;
            };
            let meta = row.meta();
            let status =
                GopeedManager::map_status(g.get("status").and_then(Value::as_str).unwrap_or(""));
            if status == "error" && row.status != "cookie_expired" {
                match self.classify_uc_error(&row, g, &meta).await {
                    Some("cookie_expired") => {
                        let db = self.db.lock().unwrap();
                        let _ = db.execute(
                            "UPDATE tasks SET status = 'cookie_expired', error = 'UC Cookie 已失效，请在设置中更新', updated_at = ?1 WHERE id = ?2",
                            params![now_iso(), row.id],
                        );
                        continue;
                    }
                    Some("retry") => {
                        if self.refresh_uc_url(&row, meta.clone()).await {
                            continue;
                        }
                    }
                    _ => {}
                }
            }
            if status == "done" && row.status != "done" {
                let name = g.get("name").and_then(Value::as_str).unwrap_or("离线任务");
                if let Err(err) = self.register_into_tree(row.id, name) {
                    let db = self.db.lock().unwrap();
                    let _ = db.execute(
                        "UPDATE tasks SET status = 'error', error = ?1, updated_at = ?2 WHERE id = ?3",
                        params![format!("文件登记失败: {}", err.message), now_iso(), row.id],
                    );
                    continue;
                }
            }
            let total = value_u64(g.get("size"))
                .or_else(|| {
                    g.get("meta")
                        .and_then(|m| m.get("res"))
                        .and_then(|r| value_u64(r.get("size")))
                })
                .unwrap_or(0);
            let mut meta = meta;
            if total > 0 && meta.get("total").is_none() {
                if let Some(obj) = meta.as_object_mut() {
                    obj.insert("total".to_string(), json!(total));
                }
                let db = self.db.lock().unwrap();
                let _ = db.execute(
                    "UPDATE tasks SET metadata = ?1 WHERE id = ?2",
                    params![
                        serde_json::to_string(&meta).unwrap_or_else(|_| "{}".to_string()),
                        row.id
                    ],
                );
            }
            let downloaded = value_u64(g.get("progress").and_then(|p| p.get("downloaded")))
                .unwrap_or(0)
                .min(total);
            let progress = if status == "done" {
                100.0
            } else if total > 0 {
                ((downloaded as f64 / total as f64 * 1000.0).round() / 10.0).min(100.0)
            } else {
                0.0
            };
            let speed = self.calculate_speed(row.id, status == "running", total, downloaded);
            let finished_at = if status == "done" {
                Some(now_iso())
            } else {
                None
            };
            let error = if status == "done" {
                String::new()
            } else if status == "error" {
                "下载失败".to_string()
            } else {
                row.error.clone()
            };
            let db = self.db.lock().unwrap();
            let _ = db.execute(
                "UPDATE tasks SET status = ?1, progress = ?2, speed = ?3,
                    finished_at = COALESCE(?4, finished_at), error = ?5, updated_at = ?6
                 WHERE id = ?7 AND (status != ?8 OR progress != ?9 OR speed != ?10 OR error != ?11)",
                params![status, progress, speed, finished_at, error, now_iso(), row.id, status, progress, speed, error],
            );
        }
        Ok(())
    }

    fn calculate_speed(&self, id: i64, running: bool, total: u64, downloaded: u64) -> i64 {
        if !running || total == 0 {
            self.speed_cache.lock().unwrap().remove(&id);
            return 0;
        }
        let now = Instant::now();
        let mut cache = self.speed_cache.lock().unwrap();
        let speed = cache
            .get(&id)
            .and_then(|prev| {
                let elapsed = now.duration_since(prev.at);
                if elapsed >= Duration::from_millis(500) && downloaded >= prev.bytes {
                    Some(((downloaded - prev.bytes) as f64 / elapsed.as_secs_f64()).round() as i64)
                } else {
                    None
                }
            })
            .unwrap_or(0);
        cache.insert(
            id,
            SpeedPoint {
                bytes: downloaded,
                at: now,
            },
        );
        speed
    }

    async fn classify_uc_error(
        &self,
        row: &TaskRow,
        g: &Value,
        meta: &Value,
    ) -> Option<&'static str> {
        let uc_meta = meta.get("uc")?;
        if !uc_meta.is_object() {
            return None;
        }
        let message = g
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_lowercase();
        if message.contains("cookie")
            || message.contains("require login")
            || message.contains("authentication failed")
            || message.contains("login required")
        {
            return Some("cookie_expired");
        }
        if ["401", "403", "404", "expired", "signature", "error code 22"]
            .iter()
            .any(|needle| message.contains(needle))
        {
            return Some("retry");
        }
        let cookie = {
            let db = self.db.lock().unwrap();
            get_uc_cookie(&db, &self.data_dir)
        };
        let probe = uc::probe_download_url(
            &self.client,
            &row.source_url,
            &cookie,
            Duration::from_secs(8),
        )
        .await;
        match probe.kind {
            "cookie_expired" => Some("cookie_expired"),
            "url_invalid" => Some("retry"),
            _ => None,
        }
    }

    /// UC 直链刷新：换新会话和 URL，删除旧 gopeed 任务后重建（保留已下载文件）。
    async fn refresh_uc_url(&self, row: &TaskRow, mut meta: Value) -> bool {
        let Some(uc_meta) = meta.get("uc").cloned() else {
            return false;
        };
        let Some(uc_obj) = uc_meta.as_object() else {
            return false;
        };
        let share_id = uc_obj.get("shareId").and_then(Value::as_str).unwrap_or("");
        let fid = uc_obj.get("fid").and_then(Value::as_str).unwrap_or("");
        if share_id.is_empty() || fid.is_empty() {
            return false;
        }
        let retry_count = value_i64(uc_obj.get("retryCount")).unwrap_or(0);
        if retry_count >= 5 {
            return false;
        }
        let last_refresh = value_i64(uc_obj.get("lastRefreshAt")).unwrap_or(0);
        if now_ms() - last_refresh < 30_000 {
            return false;
        }
        let cookie = {
            let db = self.db.lock().unwrap();
            get_uc_cookie(&db, &self.data_dir)
        };
        if cookie.is_empty() {
            let db = self.db.lock().unwrap();
            let _ = db.execute(
                "UPDATE tasks SET status = 'cookie_expired', error = '需要 UC Cookie 才能刷新下载链接', updated_at = ?1 WHERE id = ?2",
                params![now_iso(), row.id],
            );
            return true;
        }
        let share_link = uc_obj
            .get("shareLink")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| format!("https://drive.uc.cn/s/{share_id}"));
        let session = match uc::get_ctoken(&share_link, &cookie).await {
            Ok(session) => session,
            Err(err) => return self.record_refresh_error(row, meta, retry_count, err),
        };
        let old_stoken = uc_obj.get("stoken").and_then(Value::as_str).unwrap_or("");
        let old_token = uc_obj
            .get("shareFidToken")
            .and_then(Value::as_str)
            .unwrap_or("");
        let (stoken, share_fid_token, url) = match uc::get_download_url(
            &self.client,
            share_id,
            old_stoken,
            fid,
            old_token,
            &session.ctoken,
            &session.cookies,
        )
        .await
        {
            Ok(url) => (old_stoken.to_string(), old_token.to_string(), url),
            Err(_) => {
                let stoken =
                    match uc::get_stoken(&self.client, share_id, &session.ctoken, &session.cookies)
                        .await
                    {
                        Ok(v) => v,
                        Err(err) => return self.record_refresh_error(row, meta, retry_count, err),
                    };
                let files = match uc::find_files(
                    &self.client,
                    share_id,
                    &stoken,
                    &session.ctoken,
                    &session.cookies,
                    None,
                )
                .await
                {
                    Ok(v) => v,
                    Err(err) => return self.record_refresh_error(row, meta, retry_count, err),
                };
                let Some(file) = files.iter().find(|f| f.fid == fid) else {
                    return self.record_refresh_error(
                        row,
                        meta,
                        retry_count,
                        "分享文件已失效".to_string(),
                    );
                };
                let token = file.share_fid_token.clone();
                let url = match uc::get_download_url(
                    &self.client,
                    share_id,
                    &stoken,
                    fid,
                    &token,
                    &session.ctoken,
                    &session.cookies,
                )
                .await
                {
                    Ok(v) => v,
                    Err(err) => return self.record_refresh_error(row, meta, retry_count, err),
                };
                (stoken, token, url)
            }
        };
        let probe =
            uc::probe_download_url(&self.client, &url, &session.cookies, Duration::from_secs(8))
                .await;
        if probe.kind == "cookie_expired" {
            let db = self.db.lock().unwrap();
            let _ = db.execute(
                "UPDATE tasks SET status = 'cookie_expired', error = 'UC Cookie 已失效，请在设置中更新', updated_at = ?1 WHERE id = ?2",
                params![now_iso(), row.id],
            );
            return true;
        }
        if probe.kind == "url_invalid" {
            let db = self.db.lock().unwrap();
            let _ = db.execute(
                "UPDATE tasks SET error = '刷新链接后签名仍无效，请稍后重试', updated_at = ?1 WHERE id = ?2",
                params![now_iso(), row.id],
            );
            return false;
        }

        let _ = self.gopeed.remove(&row.gopeed_id, false).await;
        let headers = json!({
            "Cookie": session.cookies,
            "User-Agent": uc::UA,
            "Referer": "https://drive.uc.cn/",
            "Origin": "https://drive.uc.cn",
            "x-csrf-token": session.ctoken,
        });
        let request = json!({ "url": url, "extra": { "header": headers } });
        let mut opts = json!({ "path": row.target_dir });
        if let Some(filename) = uc_obj
            .get("filename")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
        {
            opts["name"] = json!(filename);
        }
        let gid = match self.gopeed.create_task(request, opts).await {
            Ok(v) => v,
            Err(err) => return self.record_refresh_error(row, meta, retry_count, err),
        };
        if let Some(obj) = meta.as_object_mut() {
            let mut next_uc = uc_obj.clone();
            next_uc.insert("stoken".to_string(), json!(stoken));
            next_uc.insert("shareFidToken".to_string(), json!(share_fid_token));
            next_uc.insert("retryCount".to_string(), json!(retry_count + 1));
            next_uc.insert("lastRefreshAt".to_string(), json!(now_ms()));
            obj.insert("uc".to_string(), Value::Object(next_uc));
        }
        let db = self.db.lock().unwrap();
        let _ = db.execute(
            "UPDATE tasks SET gopeed_id = ?1, status = 'queued', error = '', source_url = ?2, metadata = ?3, updated_at = ?4 WHERE id = ?5",
            params![gid, url, serde_json::to_string(&meta).unwrap_or_else(|_| "{}".to_string()), now_iso(), row.id],
        );
        true
    }

    fn record_refresh_error(
        &self,
        row: &TaskRow,
        mut meta: Value,
        retry_count: i64,
        error: String,
    ) -> bool {
        if let Some(obj) = meta.as_object_mut() {
            if let Some(uc) = obj.get_mut("uc").and_then(Value::as_object_mut) {
                uc.insert("retryCount".to_string(), json!(retry_count + 1));
                uc.insert("lastRefreshAt".to_string(), json!(now_ms()));
            }
        }
        let db = self.db.lock().unwrap();
        let _ = db.execute(
            "UPDATE tasks SET error = ?1, metadata = ?2, updated_at = ?3 WHERE id = ?4",
            params![
                format!("刷新链接失败: {error}"),
                serde_json::to_string(&meta).unwrap_or_else(|_| "{}".to_string()),
                now_iso(),
                row.id
            ],
        );
        false
    }

    /// 任务完成后把 target_dir 内容登记进文件树。
    fn register_into_tree(&self, task_id: i64, task_name: &str) -> ApiResult<()> {
        let row = self
            .get_row(task_id)?
            .ok_or_else(|| ApiError::internal("任务不存在"))?;
        let target = PathBuf::from(&row.target_dir);
        let files = files::collect_download_files(&target);
        if files.is_empty() {
            return Ok(());
        }
        let storage = self.storage.get();
        let db = self.db.lock().unwrap();
        let single = files.len() == 1
            && norm_path(files[0].parent().unwrap_or_else(|| Path::new("."))) == row.target_dir;
        if single {
            let dest = unique_path(&storage.join(files[0].file_name().unwrap_or_default()));
            move_download_file(&files[0], &dest).map_err(|e| ApiError::from_io(&e))?;
            let root_id: Option<i64> = db
                .query_row(
                    "SELECT id FROM files WHERE path = ?1",
                    [norm_path(&storage)],
                    |r| r.get(0),
                )
                .optional()
                .map_err(|e| ApiError::internal(format!("数据库错误: {e}")))?;
            files::upsert_file_row(&db, &storage, root_id, &dest)?;
        } else {
            let mut clean_name = if task_name.is_empty() {
                format!("任务-{task_id}")
            } else {
                task_name.to_string()
            };
            clean_name = clean_name
                .chars()
                .map(|c| {
                    if matches!(c, '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
                        '_'
                    } else {
                        c
                    }
                })
                .collect();
            if clean_name.is_empty() {
                clean_name = format!("任务-{task_id}");
            }
            let final_dir = unique_dir(&db, &storage, &clean_name)?;
            let ts = now_iso();
            db.execute(
                "INSERT INTO files (name, parent_id, is_dir, path, size, mime, created_at, updated_at)
                 VALUES (?1, NULL, 1, ?2, 0, '', ?3, ?4)",
                params![
                    final_dir.file_name().unwrap_or_default().to_string_lossy().to_string(),
                    norm_path(&final_dir),
                    ts,
                    ts
                ],
            )
            .map_err(|e| ApiError::internal(format!("数据库错误: {e}")))?;
            for source in files {
                let Some(relative) = relative_suffix(&row.target_dir, &norm_path(&source)) else {
                    continue;
                };
                let dest = final_dir.join(relative.replace('/', std::path::MAIN_SEPARATOR_STR));
                if let Some(parent) = dest.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| ApiError::from_io(&e))?;
                }
                move_download_file(&source, &dest).map_err(|e| ApiError::from_io(&e))?;
                files::upsert_file_row(&db, &storage, None, &dest)?;
            }
        }
        let _ = std::fs::remove_dir_all(&target);
        db.execute(
            "UPDATE tasks SET target_dir = ?1 WHERE id = ?2",
            params![norm_path(&storage), task_id],
        )
        .map_err(|e| ApiError::internal(format!("数据库错误: {e}")))?;
        Ok(())
    }
}

fn value_string(value: Option<&Value>) -> Option<String> {
    value.and_then(|v| match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    })
}

fn value_u64(value: Option<&Value>) -> Option<u64> {
    value.and_then(|v| match v {
        Value::Number(n) => n.as_u64().or_else(|| n.as_f64().map(|v| v.max(0.0) as u64)),
        Value::String(s) => s.parse::<u64>().ok(),
        _ => None,
    })
}

fn value_i64(value: Option<&Value>) -> Option<i64> {
    value.and_then(|v| match v {
        Value::Number(n) => n.as_i64().or_else(|| n.as_f64().map(|v| v as i64)),
        Value::String(s) => s.parse::<i64>().ok(),
        _ => None,
    })
}

fn clamp_number(value: Value, min: i64, max: i64) -> ApiResult<i64> {
    let n = match value {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.parse::<f64>().ok(),
        Value::Bool(v) => Some(if v { 1.0 } else { 0.0 }),
        Value::Null => Some(0.0),
        _ => None,
    }
    .filter(|v| v.is_finite())
    .ok_or_else(|| ApiError::internal("参数必须是数字"))?;
    Ok((n.round() as i64).clamp(min, max))
}

fn is_cross_device(err: &std::io::Error) -> bool {
    err.raw_os_error() == Some(17) || err.kind() == std::io::ErrorKind::CrossesDevices
}

fn move_download_file(src: &Path, dest: &Path) -> std::io::Result<()> {
    match std::fs::rename(src, dest) {
        Ok(()) => Ok(()),
        Err(err) if is_cross_device(&err) => {
            std::fs::copy(src, dest)?;
            std::fs::remove_file(src)
        }
        Err(err) => Err(err),
    }
}

fn unique_dir(db: &Connection, storage: &Path, name: &str) -> ApiResult<PathBuf> {
    let mut candidate = storage.join(name);
    let mut i = 1;
    loop {
        let path = norm_path(&candidate);
        let exists: bool = db
            .query_row("SELECT 1 FROM files WHERE path = ?1", [&path], |_r| {
                Ok(true)
            })
            .optional()
            .map_err(|e| ApiError::internal(format!("数据库错误: {e}")))?
            .unwrap_or(false);
        if !exists && !candidate.exists() {
            return Ok(candidate);
        }
        candidate = storage.join(format!("{name} ({i})"));
        i += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 数字钳制与非法值() {
        assert_eq!(clamp_number(json!(1.6), 1, 10).unwrap(), 2);
        assert_eq!(clamp_number(json!(99), 1, 10).unwrap(), 10);
        assert!(clamp_number(json!("abc"), 1, 10).is_err());
    }
}
