//! Axum HTTP API，保持现有前端路径、字段、状态码和 CORS 契约。

use std::collections::HashMap;
use std::convert::Infallible;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use axum::body::{Body, Bytes};
use axum::extract::{
    DefaultBodyLimit, FromRequest, Multipart, Path as AxumPath, Query, Request, State,
};
use axum::http::header::{self, HeaderMap, HeaderValue};
use axum::http::{Method, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use tokio_util::io::ReaderStream;

use super::crypto::{delete_setting, get_uc_cookie, has_uc_cookie, set_setting, set_uc_cookie};
use super::error::{ApiError, ApiResult};
use super::files::{self, RenamePatch};
use super::models::{download_config_json, ParentRef};
use super::tasks::{ConfigPatch, CreateTaskParams};
use super::util::{basename_of, encode_uri_component, is_previewable, mime_of, norm_path, now_ms};
use super::AppState;

const MAX_UPLOAD_FILE_SIZE: u64 = 4 * 1024 * 1024 * 1024;
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// 宽松 JSON body 提取器：没有 body 时给 Null，字段访问行为与现有 API 对齐。
/// 这样上传/命令接口不会因空 body 在 extractor 层产生无法控制的错误响应。
pub struct BodyJson(pub Value);

impl<S> FromRequest<S> for BodyJson
where
    S: Send + Sync,
{
    type Rejection = Infallible;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let bytes = Bytes::from_request(req, state).await.unwrap_or_default();
        let value = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).unwrap_or(Value::Null)
        };
        Ok(BodyJson(value))
    }
}

/// 构建完整 API Router。
pub fn build_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/api/health", get(health))
        .route("/api/settings", get(settings))
        .route("/api/settings/storage-dir", put(set_storage_dir))
        .route("/api/files", get(list_files).post(upload_files))
        .route(
            "/api/files/{id}",
            get(get_file).patch(patch_file).delete(delete_file),
        )
        .route("/api/files/{id}/ancestors", get(file_ancestors))
        .route(
            "/api/files/{id}/download",
            get(download_file).head(download_file),
        )
        .route("/api/tree", get(tree))
        .route("/api/search", get(search))
        .route("/api/dirs", post(make_dir))
        .route("/api/tmp-files", post(upload_tmp))
        .route("/api/tasks", get(list_tasks).post(create_task))
        .route(
            "/api/tasks/config",
            get(task_config).put(update_task_config),
        )
        .route("/api/tasks/{id}", get(get_task))
        .route("/api/tasks/{id}/pause", post(pause_task))
        .route("/api/tasks/{id}/resume", post(resume_task))
        .route("/api/tasks/{id}/delete", post(delete_task))
        .route("/api/uc/parse", post(uc_parse))
        .route("/api/uc/list-folder", post(uc_list_folder))
        .route("/api/uc/download", post(uc_download))
        .route(
            "/api/cookie",
            get(cookie_status).put(save_cookie).delete(clear_cookie),
        )
        .route("/api/history", get(history).delete(clear_history))
        // axum 默认 body limit 是 2MB；multipart 需要支持单文件 4GB。
        .layer(DefaultBodyLimit::max(4usize * 1024 * 1024 * 1024))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            access_log_middleware,
        ))
        .layer(middleware::from_fn(cors_middleware))
        .with_state(state)
}

async fn cors_middleware(req: Request, next: Next) -> Response {
    if req.method() == Method::OPTIONS {
        let mut response = StatusCode::NO_CONTENT.into_response();
        add_cors_headers(response.headers_mut());
        return response;
    }
    let mut response = next.run(req).await;
    add_cors_headers(response.headers_mut());
    response
}

fn add_cors_headers(headers: &mut HeaderMap) {
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_ORIGIN,
        HeaderValue::from_static("*"),
    );
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_METHODS,
        HeaderValue::from_static("GET,POST,PUT,PATCH,DELETE,OPTIONS"),
    );
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_HEADERS,
        HeaderValue::from_static("Content-Type,Range"),
    );
    headers.insert(
        header::ACCESS_CONTROL_EXPOSE_HEADERS,
        HeaderValue::from_static("Content-Range,Content-Length,Accept-Ranges"),
    );
}

async fn access_log_middleware(
    State(state): State<Arc<AppState>>,
    req: Request,
    next: Next,
) -> Response {
    let url = req
        .uri()
        .path_and_query()
        .map(|v| v.as_str())
        .unwrap_or(req.uri().path());
    state.access_log.push(req.method().as_str(), url);
    next.run(req).await
}

async fn health(State(state): State<Arc<AppState>>) -> Json<Value> {
    Json(json!({
        "ok": true,
        "gopeed": state.gopeed.ready(),
        "version": VERSION,
    }))
}

fn path_string(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

fn settings_payload(state: &AppState) -> Value {
    let storage = state.storage.get();
    let config = state.tasks.get_config();
    json!({
        "storageDir": path_string(&storage),
        "defaultStorageDir": path_string(&state.default_storage),
        "dataDir": path_string(&state.paths.data_dir),
        "gopeedDir": path_string(&state.paths.gopeed_dir),
        "gopeed": {
            "running": state.gopeed.ready(),
            "port": state.gopeed.port(),
            "base": state.gopeed.base(),
        },
        "download": download_config_json(&config),
    })
}

async fn settings(State(state): State<Arc<AppState>>) -> Json<Value> {
    Json(settings_payload(&state))
}

async fn set_storage_dir(
    State(state): State<Arc<AppState>>,
    BodyJson(body): BodyJson,
) -> ApiResult<Json<Value>> {
    let object = body.as_object();
    let raw = object
        .and_then(|o| o.get("dir"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    let move_files = object
        .and_then(|o| o.get("moveFiles"))
        .map(|v| v != &Value::Bool(false))
        .unwrap_or(true);
    let current = state.storage.get();
    let target = if raw.is_empty() {
        state.default_storage.clone()
    } else {
        resolve_path(&raw)
    };
    if norm_path(&target).eq_ignore_ascii_case(&norm_path(&current)) {
        let mut payload = settings_payload(&state);
        if let Some(obj) = payload.as_object_mut() {
            obj.insert("changed".to_string(), json!(false));
        }
        return Ok(Json(payload));
    }

    std::fs::create_dir_all(&target).map_err(|e| ApiError::from_io(&e))?;
    let probe = target.join(format!(".ucd2-write-test-{}-{}", now_ms(), random_suffix()));
    std::fs::write(&probe, b"ok").map_err(|e| ApiError::from_io(&e))?;
    let _ = std::fs::remove_file(&probe);

    let moved = if move_files {
        let db = state.db.lock().unwrap();
        files::move_storage_dir(&db, &current, &target)?
    } else {
        0
    };
    {
        let db = state.db.lock().unwrap();
        set_setting(&db, "storage_dir", &raw);
    }
    state.storage.set(target);
    ensure_dirs(&state)?;

    let mut payload = settings_payload(&state);
    if let Some(obj) = payload.as_object_mut() {
        obj.insert("changed".to_string(), json!(true));
        obj.insert("movedFiles".to_string(), json!(moved));
    }
    Ok(Json(payload))
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

fn random_suffix() -> String {
    use rand::Rng;
    format!("{:x}", rand::thread_rng().gen::<u64>())
}

fn ensure_dirs(state: &AppState) -> ApiResult<()> {
    for dir in [
        state.paths.data_dir.clone(),
        state.paths.db_dir.clone(),
        state.paths.gopeed_dir.clone(),
        state.storage.get(),
        state.storage.offline_dir(),
        state.paths.data_dir.join("tmp"),
    ] {
        std::fs::create_dir_all(dir).map_err(|e| ApiError::from_io(&e))?;
    }
    Ok(())
}

fn parent_from_query(query: &HashMap<String, String>) -> ParentRef {
    match query.get("parent") {
        None => ParentRef::Root,
        Some(value) if value == "root" => ParentRef::Root,
        Some(value) => value
            .parse::<i64>()
            .map(ParentRef::Id)
            .unwrap_or(ParentRef::Invalid),
    }
}

async fn list_files(
    State(state): State<Arc<AppState>>,
    Query(query): Query<HashMap<String, String>>,
) -> ApiResult<Json<Vec<super::models::FileDto>>> {
    let parent = parent_from_query(&query);
    let storage = state.storage.get();
    let db = state.db.lock().unwrap();
    Ok(Json(files::list_dir(&db, &storage, &parent)?))
}

async fn get_file(
    State(state): State<Arc<AppState>>,
    AxumPath(id): AxumPath<i64>,
) -> ApiResult<Json<super::models::FileDto>> {
    let db = state.db.lock().unwrap();
    files::get_row(&db, id)
        .map(|row| Json(row.dto()))
        .ok_or_else(|| ApiError::not_found("文件不存在"))
}

async fn file_ancestors(
    State(state): State<Arc<AppState>>,
    AxumPath(id): AxumPath<i64>,
) -> ApiResult<Json<Vec<super::models::FileDto>>> {
    let db = state.db.lock().unwrap();
    match files::ancestors(&db, id)? {
        Some(chain) => Ok(Json(chain)),
        None => Err(ApiError::not_found("文件不存在")),
    }
}

async fn search(
    State(state): State<Arc<AppState>>,
    Query(query): Query<HashMap<String, String>>,
) -> ApiResult<Json<Vec<super::models::FileDto>>> {
    let q = query
        .get("q")
        .cloned()
        .unwrap_or_default()
        .trim()
        .to_string();
    if q.is_empty() {
        return Ok(Json(Vec::new()));
    }
    let db = state.db.lock().unwrap();
    Ok(Json(files::search(&db, &q, 50)?))
}

async fn tree(State(state): State<Arc<AppState>>) -> ApiResult<Json<Vec<super::models::TreeNode>>> {
    let db = state.db.lock().unwrap();
    Ok(Json(files::tree(&db)?))
}

#[derive(Debug, Clone, Copy)]
enum ParsedRange {
    None,
    Invalid,
    Partial { start: u64, end: u64 },
}

fn parse_range(value: Option<&HeaderValue>, size: u64) -> ParsedRange {
    let Some(value) = value.and_then(|v| v.to_str().ok()) else {
        return ParsedRange::None;
    };
    let Some(raw) = value.trim().strip_prefix("bytes=") else {
        return ParsedRange::None;
    };
    let Some((start_raw, end_raw)) = raw.split_once('-') else {
        return ParsedRange::None;
    };
    if start_raw.is_empty() && end_raw.is_empty() {
        return ParsedRange::None;
    }
    let (start, end) = if start_raw.is_empty() {
        let Some(suffix) = end_raw.parse::<u64>().ok() else {
            return ParsedRange::None;
        };
        (size.saturating_sub(suffix), size.saturating_sub(1))
    } else {
        let Some(start) = start_raw.parse::<u64>().ok() else {
            return ParsedRange::None;
        };
        let end = if end_raw.is_empty() {
            size.saturating_sub(1)
        } else {
            match end_raw.parse::<u64>() {
                Ok(value) => value.min(size.saturating_sub(1)),
                Err(_) => return ParsedRange::None,
            }
        };
        (start, end)
    };
    if start > end || start >= size {
        ParsedRange::Invalid
    } else {
        ParsedRange::Partial { start, end }
    }
}

async fn download_file(
    State(state): State<Arc<AppState>>,
    AxumPath(id): AxumPath<i64>,
    Query(query): Query<HashMap<String, String>>,
    headers: HeaderMap,
    method: Method,
) -> ApiResult<Response> {
    let row = {
        let db = state.db.lock().unwrap();
        files::get_row(&db, id).ok_or_else(|| ApiError::not_found("文件不存在"))?
    };
    if row.is_dir {
        return Err(ApiError::bad_request("目录不可下载"));
    }
    let path = PathBuf::from(&row.path);
    if !path.exists() {
        return Err(ApiError::not_found("文件在磁盘上不存在"));
    }
    let stat = tokio::fs::metadata(&path)
        .await
        .map_err(|e| ApiError::from_io(&e))?;
    let size = stat.len();
    let preview =
        query.get("preview").map(|v| v == "1").unwrap_or(false) || is_previewable(&row.mime);
    let disposition = if preview { "inline" } else { "attachment" };
    let name = path
        .file_name()
        .map(|v| v.to_string_lossy().to_string())
        .unwrap_or_else(|| basename_of(&row.path));
    let encoded = encode_uri_component(&name);
    let range = parse_range(headers.get(header::RANGE), size);
    if matches!(range, ParsedRange::Invalid) {
        let mut response = Response::new(Body::empty());
        *response.status_mut() = StatusCode::RANGE_NOT_SATISFIABLE;
        response.headers_mut().insert(
            header::CONTENT_RANGE,
            HeaderValue::from_str(&format!("bytes */{size}")).unwrap(),
        );
        return Ok(response);
    }

    let mut response_headers = HeaderMap::new();
    response_headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(if row.mime.is_empty() {
            mime_of(&name)
        } else {
            &row.mime
        })
        .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream")),
    );
    response_headers.insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    response_headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!("{disposition}; filename*=UTF-8''{encoded}"))
            .unwrap_or_else(|_| HeaderValue::from_static("attachment")),
    );
    response_headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, max-age=3600"),
    );

    let (status, content_length, start) = match range {
        ParsedRange::Partial { start, end } => {
            response_headers.insert(
                header::CONTENT_RANGE,
                HeaderValue::from_str(&format!("bytes {start}-{end}/{size}")).unwrap(),
            );
            (StatusCode::PARTIAL_CONTENT, end - start + 1, start)
        }
        ParsedRange::None => (StatusCode::OK, size, 0),
        ParsedRange::Invalid => unreachable!(),
    };
    response_headers.insert(
        header::CONTENT_LENGTH,
        HeaderValue::from_str(&content_length.to_string()).unwrap(),
    );

    if method == Method::HEAD {
        return Ok((status, response_headers, Body::empty()).into_response());
    }

    let mut file = tokio::fs::File::open(&path)
        .await
        .map_err(|e| ApiError::from_io(&e))?;
    if start > 0 {
        file.seek(std::io::SeekFrom::Start(start))
            .await
            .map_err(|e| ApiError::from_io(&e))?;
    }
    let stream = ReaderStream::new(file.take(content_length));
    let response = (status, response_headers, Body::from_stream(stream)).into_response();
    // HEAD 分支需要同样 headers；正常 GET 已在 tuple 中携带。
    Ok(response)
}

async fn make_dir(
    State(state): State<Arc<AppState>>,
    BodyJson(body): BodyJson,
) -> ApiResult<(StatusCode, Json<super::models::FileDto>)> {
    let object = body.as_object();
    let Some(name) = object.and_then(|o| o.get("name")).map(value_to_string) else {
        return Err(ApiError::bad_request("缺少目录名"));
    };
    let parent = ParentRef::parse(object.and_then(|o| o.get("parent")));
    let storage = state.storage.get();
    let db = state.db.lock().unwrap();
    let dto = files::mkdir(&db, &storage, &name, &parent)?;
    Ok((StatusCode::CREATED, Json(dto)))
}

async fn patch_file(
    State(state): State<Arc<AppState>>,
    AxumPath(id): AxumPath<i64>,
    BodyJson(body): BodyJson,
) -> ApiResult<Json<super::models::FileDto>> {
    let object = body.as_object().cloned().unwrap_or_default();
    if !object.contains_key("name") && !object.contains_key("parent") {
        return Err(ApiError::bad_request("没有可更新的字段"));
    }
    let name = object.get("name").map(value_to_string);
    let parent = if object.contains_key("parent") {
        match ParentRef::parse(object.get("parent")) {
            ParentRef::Root => Some(None),
            ParentRef::Id(id) => Some(Some(id)),
            ParentRef::Invalid => return Err(ApiError::not_found("目录不存在")),
        }
    } else {
        None
    };
    let storage = state.storage.get();
    let db = state.db.lock().unwrap();
    Ok(Json(files::rename(
        &db,
        &storage,
        id,
        RenamePatch { name, parent },
    )?))
}

async fn delete_file(
    State(state): State<Arc<AppState>>,
    AxumPath(id): AxumPath<i64>,
) -> ApiResult<Json<Value>> {
    let storage = state.storage.get();
    files::remove(&state.db, &storage, id).await?;
    Ok(Json(json!({ "ok": true })))
}

async fn upload_files(
    State(state): State<Arc<AppState>>,
    mut multipart: Multipart,
) -> ApiResult<Json<Vec<super::models::FileDto>>> {
    let mut parent_raw: Option<String> = None;
    let mut files_to_register: Vec<(PathBuf, String)> = Vec::new();
    while let Some(mut field) = multipart
        .next_field()
        .await
        .map_err(|e| ApiError::internal(format!("上传失败: {e}")))?
    {
        let name = field.name().map(str::to_string);
        match name.as_deref() {
            Some("parent") => {
                parent_raw = Some(
                    field
                        .text()
                        .await
                        .map_err(|e| ApiError::internal(format!("上传失败: {e}")))?,
                );
            }
            Some("files") => {
                let original = field
                    .file_name()
                    .map(str::to_string)
                    .unwrap_or_else(|| "upload".to_string());
                let tmp =
                    save_multipart_field(&mut field, &state.paths.data_dir, &original).await?;
                files_to_register.push((tmp, original));
                if files_to_register.len() > 100 {
                    return Err(ApiError::internal("Unexpected field"));
                }
            }
            _ => {}
        }
    }
    if files_to_register.is_empty() {
        return Err(ApiError::bad_request("未收到文件"));
    }
    let parent = parse_parent_string(parent_raw.as_deref());
    let storage = state.storage.get();
    let db = state.db.lock().unwrap();
    let mut result = Vec::new();
    for (tmp, original) in files_to_register {
        match files::register_upload(&db, &storage, &parent, &tmp, &original) {
            Ok(dto) => result.push(dto),
            Err(err) => return Err(err),
        }
    }
    Ok(Json(result))
}

async fn upload_tmp(
    State(state): State<Arc<AppState>>,
    mut multipart: Multipart,
) -> ApiResult<(StatusCode, Json<Value>)> {
    let mut saved: Option<(PathBuf, String)> = None;
    while let Some(mut field) = multipart
        .next_field()
        .await
        .map_err(|e| ApiError::internal(format!("上传失败: {e}")))?
    {
        if field.name() == Some("file") && saved.is_none() {
            let original = field
                .file_name()
                .map(str::to_string)
                .unwrap_or_else(|| "upload".to_string());
            let tmp = save_multipart_field(&mut field, &state.paths.data_dir, &original).await?;
            saved = Some((tmp, original));
        }
    }
    let Some((tmp, original)) = saved else {
        return Err(ApiError::bad_request("未收到文件"));
    };
    if !original.to_lowercase().ends_with(".torrent") {
        let _ = std::fs::remove_file(tmp);
        return Err(ApiError::bad_request("仅支持 .torrent 文件"));
    }
    let dir = state.paths.data_dir.join("tmp").join("torrents");
    std::fs::create_dir_all(&dir).map_err(|e| ApiError::from_io(&e))?;
    let file_name = tmp
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| format!("{}-{}.torrent", now_ms(), random_suffix()));
    let dest = dir.join(file_name);
    std::fs::rename(&tmp, &dest).map_err(|e| ApiError::from_io(&e))?;
    let name = dest
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    Ok((StatusCode::CREATED, Json(json!({ "name": name }))))
}

async fn save_multipart_field(
    field: &mut axum::extract::multipart::Field<'_>,
    data_dir: &Path,
    original: &str,
) -> ApiResult<PathBuf> {
    let dir = data_dir.join("tmp");
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| ApiError::from_io(&e))?;
    let path = dir.join(format!(
        "{}-{}-{}",
        now_ms(),
        random_suffix(),
        basename_of(original)
    ));
    let mut file = tokio::fs::File::create(&path)
        .await
        .map_err(|e| ApiError::from_io(&e))?;
    let mut total = 0u64;
    while let Some(chunk) = field
        .chunk()
        .await
        .map_err(|e| ApiError::internal(format!("上传失败: {e}")))?
    {
        total = total.saturating_add(chunk.len() as u64);
        if total > MAX_UPLOAD_FILE_SIZE {
            let _ = tokio::fs::remove_file(&path).await;
            return Err(ApiError::too_large("文件过大"));
        }
        file.write_all(&chunk)
            .await
            .map_err(|e| ApiError::from_io(&e))?;
    }
    file.flush().await.map_err(|e| ApiError::from_io(&e))?;
    Ok(path)
}

fn parse_parent_string(raw: Option<&str>) -> ParentRef {
    match raw {
        None | Some("root") => ParentRef::Root,
        Some(value) => value
            .trim()
            .parse::<i64>()
            .map(ParentRef::Id)
            .unwrap_or(ParentRef::Invalid),
    }
}

fn value_to_string(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Null => "null".to_string(),
        _ => value.to_string(),
    }
}

fn value_i64(value: Option<&Value>) -> Option<i64> {
    value.and_then(|v| match v {
        Value::Number(n) => n.as_i64().or_else(|| n.as_f64().map(|x| x as i64)),
        Value::String(s) => s.parse::<i64>().ok(),
        _ => None,
    })
}

async fn list_tasks(
    State(state): State<Arc<AppState>>,
) -> ApiResult<Json<Vec<super::models::TaskDto>>> {
    Ok(Json(state.tasks.list()?))
}

async fn create_task(
    State(state): State<Arc<AppState>>,
    BodyJson(body): BodyJson,
) -> ApiResult<(StatusCode, Json<super::models::TaskDto>)> {
    let object = body.as_object();
    let source = object
        .and_then(|o| o.get("source"))
        .and_then(Value::as_str)
        .unwrap_or("url")
        .to_string();
    let params = CreateTaskParams {
        source,
        url: object.and_then(|o| o.get("url")).map(value_to_string),
        torrent_id: value_i64(object.and_then(|o| o.get("torrentId"))),
        torrent_name: object
            .and_then(|o| o.get("torrentName"))
            .map(value_to_string),
        filename: object.and_then(|o| o.get("filename")).map(value_to_string),
        headers: object.and_then(|o| o.get("headers")).cloned(),
        uc: object.and_then(|o| o.get("uc")).cloned(),
        connections: value_i64(object.and_then(|o| o.get("connections"))),
    };
    let task = state.tasks.create(params).await?;
    Ok((StatusCode::CREATED, Json(task)))
}

async fn task_config(State(state): State<Arc<AppState>>) -> ApiResult<Json<Value>> {
    Ok(Json(download_config_json(&state.tasks.get_config())))
}

async fn update_task_config(
    State(state): State<Arc<AppState>>,
    BodyJson(body): BodyJson,
) -> ApiResult<Json<Value>> {
    let object = body.as_object();
    if object.map(|o| o.is_empty()).unwrap_or(true) {
        return Err(ApiError::bad_request("没有可更新的参数"));
    }
    let result = state
        .tasks
        .set_config(ConfigPatch {
            uc_connections: object.and_then(|o| o.get("ucConnections")).cloned(),
            http_connections: object.and_then(|o| o.get("httpConnections")).cloned(),
            max_running: object.and_then(|o| o.get("maxRunning")).cloned(),
        })
        .await?;
    Ok(Json(download_config_json(&result)))
}

async fn get_task(
    State(state): State<Arc<AppState>>,
    AxumPath(id): AxumPath<i64>,
) -> ApiResult<Json<super::models::TaskDto>> {
    state
        .tasks
        .get(id)?
        .map(Json)
        .ok_or_else(|| ApiError::not_found("任务不存在"))
}

async fn pause_task(
    State(state): State<Arc<AppState>>,
    AxumPath(id): AxumPath<i64>,
) -> ApiResult<Json<super::models::TaskDto>> {
    Ok(Json(state.tasks.pause(id).await?))
}

async fn resume_task(
    State(state): State<Arc<AppState>>,
    AxumPath(id): AxumPath<i64>,
) -> ApiResult<Json<super::models::TaskDto>> {
    Ok(Json(state.tasks.resume(id).await?))
}

async fn delete_task(
    State(state): State<Arc<AppState>>,
    AxumPath(id): AxumPath<i64>,
    BodyJson(body): BodyJson,
) -> ApiResult<Json<Value>> {
    let force = body
        .as_object()
        .and_then(|o| o.get("force"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    state.tasks.remove(id, force).await?;
    Ok(Json(json!({ "ok": true })))
}

async fn uc_parse(
    State(state): State<Arc<AppState>>,
    BodyJson(body): BodyJson,
) -> ApiResult<Json<Value>> {
    let object = body.as_object();
    let Some(share_link) = object
        .and_then(|o| o.get("shareLink"))
        .and_then(Value::as_str)
    else {
        return Err(ApiError::bad_request("缺少分享链接"));
    };
    let supplied = object
        .and_then(|o| o.get("cookie"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let saved = {
        let db = state.db.lock().unwrap();
        get_uc_cookie(&db, &state.paths.data_dir)
    };
    let cookie = if supplied.is_empty() {
        saved
    } else {
        supplied.to_string()
    };
    let parsed = super::uc::parse(&state.client, share_link, &cookie)
        .await
        .map_err(ApiError::internal)?;
    let mut value = serde_json::to_value(parsed).map_err(|e| ApiError::internal(e.to_string()))?;
    if let Some(obj) = value.as_object_mut() {
        obj.insert("cookieUsed".to_string(), json!(!cookie.is_empty()));
    }
    Ok(Json(value))
}

async fn uc_list_folder(
    State(state): State<Arc<AppState>>,
    BodyJson(body): BodyJson,
) -> ApiResult<Json<Value>> {
    let object = body.as_object();
    let share_id = object
        .and_then(|o| o.get("shareId"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let session = object
        .and_then(|o| o.get("session"))
        .and_then(Value::as_object);
    let stoken = session
        .and_then(|s| s.get("stoken"))
        .and_then(Value::as_str)
        .unwrap_or("");
    if share_id.is_empty() || stoken.is_empty() {
        return Err(ApiError::bad_request("缺少目录参数"));
    }
    let pdir = object
        .and_then(|o| o.get("pdirFid"))
        .and_then(Value::as_str);
    let ctoken = session
        .and_then(|s| s.get("ctoken"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let cookies = session
        .and_then(|s| s.get("cookies"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let files = super::uc::list_folder(&state.client, share_id, stoken, pdir, ctoken, cookies)
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(json!({ "files": files })))
}

async fn uc_download(
    State(state): State<Arc<AppState>>,
    BodyJson(body): BodyJson,
) -> ApiResult<(StatusCode, Json<super::models::TaskDto>)> {
    let object = body.as_object();
    let share_id = object
        .and_then(|o| o.get("shareId"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let fid = object
        .and_then(|o| o.get("fid"))
        .and_then(Value::as_str)
        .unwrap_or("");
    if share_id.is_empty() || fid.is_empty() {
        return Err(ApiError::bad_request("缺少下载参数"));
    }
    let stoken = object
        .and_then(|o| o.get("stoken"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let share_fid_token = object
        .and_then(|o| o.get("shareFidToken"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let ctoken = object
        .and_then(|o| o.get("ctoken"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let cookies = object
        .and_then(|o| o.get("cookies"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let url = super::uc::resolve_download(
        &state.client,
        share_id,
        stoken,
        fid,
        share_fid_token,
        ctoken,
        cookies,
    )
    .await
    .map_err(ApiError::internal)?;
    let probe =
        super::uc::probe_download_url(&state.client, &url, cookies, Duration::from_secs(8)).await;
    if probe.kind == "cookie_expired" {
        return Err(ApiError::with_kind(
            403,
            "UC Cookie 已失效，请在设置中更新后重试",
            "cookie_expired",
        ));
    }
    if probe.kind == "url_invalid" {
        return Err(ApiError::with_kind(
            502,
            "直链校验失败（签名无效），请重试",
            "url_invalid",
        ));
    }
    let headers = json!({
        "Cookie": cookies,
        "User-Agent": super::uc::UA,
        "Referer": "https://drive.uc.cn/",
        "Origin": "https://drive.uc.cn",
        "x-csrf-token": ctoken,
    });
    let uc_meta = json!({
        "shareId": share_id,
        "stoken": stoken,
        "fid": fid,
        "shareFidToken": share_fid_token,
        "shareLink": object.and_then(|o| o.get("shareLink")).and_then(Value::as_str).unwrap_or(""),
        "filename": object.and_then(|o| o.get("filename")).and_then(Value::as_str).unwrap_or(""),
        "size": value_i64(object.and_then(|o| o.get("size"))).unwrap_or(0),
        "retryCount": 0,
        "lastRefreshAt": 0,
    });
    let task = state
        .tasks
        .create(CreateTaskParams {
            source: "uc".to_string(),
            url: Some(url),
            torrent_id: None,
            torrent_name: None,
            filename: object.and_then(|o| o.get("filename")).map(value_to_string),
            headers: Some(headers),
            uc: Some(uc_meta),
            connections: value_i64(object.and_then(|o| o.get("connections"))),
        })
        .await?;
    Ok((StatusCode::CREATED, Json(task)))
}

async fn cookie_status(State(state): State<Arc<AppState>>) -> ApiResult<Json<Value>> {
    let db = state.db.lock().unwrap();
    Ok(Json(json!({ "hasCookie": has_uc_cookie(&db) })))
}

async fn save_cookie(
    State(state): State<Arc<AppState>>,
    BodyJson(body): BodyJson,
) -> ApiResult<Json<Value>> {
    let cookie = body
        .as_object()
        .and_then(|o| o.get("cookie"))
        .and_then(Value::as_str)
        .unwrap_or("");
    if cookie.trim().is_empty() {
        return Err(ApiError::bad_request("Cookie 不能为空"));
    }
    let db = state.db.lock().unwrap();
    set_uc_cookie(&db, &state.paths.data_dir, cookie);
    Ok(Json(json!({ "ok": true })))
}

async fn clear_cookie(State(state): State<Arc<AppState>>) -> ApiResult<Json<Value>> {
    let db = state.db.lock().unwrap();
    delete_setting(&db, "uc_cookie");
    Ok(Json(json!({ "ok": true })))
}

async fn history(
    State(state): State<Arc<AppState>>,
) -> ApiResult<Json<Vec<super::models::TaskDto>>> {
    Ok(Json(state.tasks.history()?))
}

async fn clear_history(State(state): State<Arc<AppState>>) -> ApiResult<Json<Value>> {
    Ok(Json(json!({ "deleted": state.tasks.clear_history()? })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 范围解析() {
        assert!(matches!(parse_range(None, 20), ParsedRange::None));
        assert!(matches!(
            parse_range(Some(&HeaderValue::from_static("bytes=2-5")), 20),
            ParsedRange::Partial { start: 2, end: 5 }
        ));
        assert!(matches!(
            parse_range(Some(&HeaderValue::from_static("bytes=-4")), 20),
            ParsedRange::Partial { start: 16, end: 19 }
        ));
        assert!(matches!(
            parse_range(Some(&HeaderValue::from_static("bytes=50-60")), 20),
            ParsedRange::Invalid
        ));
    }
}
