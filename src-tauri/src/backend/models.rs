//! HTTP DTO（与前端 types/index.ts 契约一一对应）。

use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Serialize)]
pub struct FileDto {
    pub id: i64,
    pub name: String,
    pub parent_id: Option<i64>,
    pub is_dir: bool,
    pub path: String,
    pub size: i64,
    pub mime: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TaskDto {
    pub id: i64,
    pub gopeed_id: String,
    pub source: String,
    pub source_url: String,
    pub status: String,
    pub progress: f64,
    pub speed: i64,
    pub error: String,
    pub target_dir: String,
    pub metadata: Value,
    pub created_at: String,
    pub updated_at: String,
    pub finished_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TreeNode {
    pub id: i64,
    pub name: String,
    pub path: String,
    pub children: Vec<TreeNode>,
}

/// UC 分享内文件条目（对应前端 UcFile）
#[derive(Debug, Clone, Serialize)]
pub struct UcFileDto {
    pub fid: String,
    pub name: String,
    pub size: i64,
    pub file: bool,
    pub format_type: String,
    pub share_fid_token: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DownloadConfigDto {
    pub uc_connections: i64,
    pub http_connections: i64,
    pub max_running: i64,
}

// serde 用 camelCase 键输出（downloadConfig: ucConnections / httpConnections / maxRunning）
pub fn download_config_json(cfg: &DownloadConfigDto) -> Value {
    serde_json::json!({
        "ucConnections": cfg.uc_connections,
        "httpConnections": cfg.http_connections,
        "maxRunning": cfg.max_running,
    })
}

/// 请求体中的 parent 字段解析结果：
/// Root = null/'root'/缺失；Id(n) = 数字；Invalid = 其他（NaN 语义）
#[derive(Debug, Clone, PartialEq)]
pub enum ParentRef {
    Root,
    Id(i64),
    Invalid,
}

impl ParentRef {
    /// parent == null || parent === 'root' ? null : Number(parent)
    pub fn parse(v: Option<&Value>) -> Self {
        match v {
            None | Some(Value::Null) => ParentRef::Root,
            Some(Value::String(s)) if s == "root" => ParentRef::Root,
            Some(Value::String(s)) => s
                .trim()
                .parse::<i64>()
                .map(ParentRef::Id)
                .unwrap_or(ParentRef::Invalid),
            Some(Value::Number(n)) => n.as_i64().map(ParentRef::Id).unwrap_or(ParentRef::Invalid),
            Some(_) => ParentRef::Invalid,
        }
    }
}
