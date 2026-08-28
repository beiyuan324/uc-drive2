//! 通用工具：
//! 路径规范化、唯一化、越界防护、MIME 推断、目录扫描、JS encodeURIComponent 等。

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use percent_encoding::{percent_encode, AsciiSet, NON_ALPHANUMERIC};

use super::error::{ApiError, ApiResult};

/// 当前时间 ISO 8601（毫秒 + Z），对标 new Date().toISOString()
pub fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

/// 当前毫秒时间戳
pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// 路径统一用正斜杠存储（Windows 下也如此），FS 操作前再还原
pub fn norm_path(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}

/// 规范化字符串形式的路径（已含正斜杠）
pub fn norm_str(p: &str) -> String {
    p.replace('\\', "/")
}

/// 判断 target 是否位于 root 内（大小写不敏感，Windows 语义）。
/// 返回 target 相对 root 的后缀（target==root 时为 ""）。
pub fn relative_suffix(root: &str, target: &str) -> Option<String> {
    let r = norm_str(root).trim_end_matches('/').to_lowercase();
    let t = norm_str(target).to_lowercase();
    if r.is_empty() {
        return Some(target.to_string());
    }
    if t == r {
        return Some(String::new());
    }
    if t.starts_with(&r) && t.as_bytes().get(r.len()) == Some(&b'/') {
        Some(target[r.len() + 1..].to_string())
    } else {
        None
    }
}

/// 防止路径穿越：确保目标仍在根目录内（EPERM → 400）
pub fn assert_inside(root: &Path, target: &Path) -> ApiResult<()> {
    let r = &root.to_string_lossy();
    let t = &target.to_string_lossy();
    if relative_suffix(r, t).is_none() {
        return Err(ApiError::bad_request("非法路径"));
    }
    Ok(())
}

/// 若重名则追加 (1)、(2)…，返回可用且唯一的路径（对标 uniquePath）
pub fn unique_path(file_path: &Path) -> PathBuf {
    if !file_path.exists() {
        return file_path.to_path_buf();
    }
    let dir = file_path.parent().unwrap_or(Path::new("."));
    let stem = file_path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let ext = file_path
        .extension()
        .map(|s| format!(".{}", s.to_string_lossy()))
        .unwrap_or_default();
    for i in 1.. {
        let candidate = dir.join(format!("{} ({}){}", stem, i, ext));
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!()
}

/// JS encodeURIComponent 兼容（不转义 A-Za-z0-9 -_.!~*'()，其余 UTF-8 百分号编码）
const JS_URI_SET: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'_')
    .remove(b'.')
    .remove(b'!')
    .remove(b'~')
    .remove(b'*')
    .remove(b'\'')
    .remove(b'(')
    .remove(b')');

pub fn encode_uri_component(s: &str) -> String {
    percent_encode(s.as_bytes(), JS_URI_SET).to_string()
}

/// multipart 文件名取 basename（对标 multer 的 path.basename 处理）
pub fn basename_of(name: &str) -> String {
    let name = name.replace('\\', "/");
    name.rsplit('/').next().unwrap_or(name.as_str()).to_string()
}

const MIME_MAP: &[(&str, &str)] = &[
    (".txt", "text/plain"),
    (".md", "text/markdown"),
    (".json", "application/json"),
    (".html", "text/html"),
    (".htm", "text/html"),
    (".css", "text/css"),
    (".js", "text/javascript"),
    (".mjs", "text/javascript"),
    (".cjs", "text/javascript"),
    (".ts", "text/typescript"),
    (".xml", "application/xml"),
    (".csv", "text/csv"),
    (".log", "text/plain"),
    (".ini", "text/plain"),
    (".yml", "text/yaml"),
    (".yaml", "text/yaml"),
    (".pdf", "application/pdf"),
    (".png", "image/png"),
    (".jpg", "image/jpeg"),
    (".jpeg", "image/jpeg"),
    (".gif", "image/gif"),
    (".webp", "image/webp"),
    (".svg", "image/svg+xml"),
    (".bmp", "image/bmp"),
    (".ico", "image/x-icon"),
    (".avif", "image/avif"),
    (".heic", "image/heic"),
    (".mp4", "video/mp4"),
    (".webm", "video/webm"),
    (".mkv", "video/x-matroska"),
    (".mov", "video/quicktime"),
    (".avi", "video/x-msvideo"),
    (".m4v", "video/x-m4v"),
    (".mp3", "audio/mpeg"),
    (".wav", "audio/wav"),
    (".flac", "audio/flac"),
    (".aac", "audio/aac"),
    (".ogg", "audio/ogg"),
    (".m4a", "audio/mp4"),
    (".opus", "audio/opus"),
    (".zip", "application/zip"),
    (".rar", "application/vnd.rar"),
    (".7z", "application/x-7z-compressed"),
    (".tar", "application/x-tar"),
    (".gz", "application/gzip"),
    (".bz2", "application/x-bzip2"),
    (".xz", "application/x-xz"),
    (".torrent", "application/x-bittorrent"),
    (".exe", "application/x-msdownload"),
    (".msi", "application/x-msi"),
    (".dll", "application/x-msdownload"),
    (".apk", "application/vnd.android.package-archive"),
    (".iso", "application/x-iso9660-image"),
    (".doc", "application/msword"),
    (
        ".docx",
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
    ),
    (".xls", "application/vnd.ms-excel"),
    (
        ".xlsx",
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
    ),
    (".ppt", "application/vnd.ms-powerpoint"),
    (
        ".pptx",
        "application/vnd.openxmlformats-officedocument.presentationml.presentation",
    ),
];

pub fn mime_of(name: &str) -> &'static str {
    let lower = name.to_lowercase();
    let ext = lower.rsplit('.').next().unwrap_or("");
    if ext.is_empty() || !lower.contains('.') {
        return "application/octet-stream";
    }
    let dot_ext = format!(".{}", ext);
    MIME_MAP
        .iter()
        .find(|(k, _)| *k == dot_ext)
        .map(|(_, v)| *v)
        .unwrap_or("application/octet-stream")
}

pub fn is_previewable(mime: &str) -> bool {
    mime.starts_with("image/")
        || mime.starts_with("video/")
        || mime.starts_with("audio/")
        || mime == "text/plain"
        || mime == "text/markdown"
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct DirEntryInfo {
    pub name: String,
    #[serde(rename = "isDir")]
    pub is_dir: bool,
    pub size: u64,
}

/// 目录扫描：返回 [{name, isDir, size}]，跳过隐藏文件，目录优先 + 名称排序。
/// 目录 size 恒为 0 —— 不递归计算子树大小（性能决策，见 HANDOVER）。
pub fn scan_dir(dir_path: &Path) -> Vec<DirEntryInfo> {
    let mut out = Vec::new();
    let entries = match std::fs::read_dir(dir_path) {
        Ok(e) => e,
        Err(_) => return out,
    };
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        let mut size = 0u64;
        let mut is_dir = false;
        if let Ok(ft) = e.file_type() {
            is_dir = ft.is_dir();
            if ft.is_file() {
                size = e.metadata().map(|m| m.len()).unwrap_or(0);
            }
        }
        out.push(DirEntryInfo { name, is_dir, size });
    }
    out.sort_by(|a, b| match (a.is_dir, b.is_dir) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.name.cmp(&b.name),
    });
    out
}

/// 递归收集目录下所有文件的绝对路径
pub fn walk_files(dir: &Path) -> Vec<PathBuf> {
    let mut list = Vec::new();
    walk_inner(dir, &mut list);
    list
}

fn walk_inner(dir: &Path, list: &mut Vec<PathBuf>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            walk_inner(&p, list);
        } else if p.is_file() {
            list.push(p);
        }
    }
}
