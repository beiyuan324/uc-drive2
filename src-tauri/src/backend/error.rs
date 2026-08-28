//! HTTP 层统一错误：
//! - ENOENT → 404；EBUSY/EPERM/EINVAL/EACCES → 400；LIMIT_FILE_SIZE → 413；其余 → 500。
//! 响应体：{"error": "..."}（可选 "kind" 供前端区分 UC Cookie 失效等场景）。

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

#[derive(Debug, Clone)]
pub struct ApiError {
    pub status: u16,
    pub message: String,
    pub kind: Option<String>,
}

impl ApiError {
    pub fn not_found(msg: impl Into<String>) -> Self {
        ApiError {
            status: 404,
            message: msg.into(),
            kind: None,
        }
    }

    pub fn bad_request(msg: impl Into<String>) -> Self {
        ApiError {
            status: 400,
            message: msg.into(),
            kind: None,
        }
    }

    pub fn too_large(msg: impl Into<String>) -> Self {
        ApiError {
            status: 413,
            message: msg.into(),
            kind: None,
        }
    }

    /// 业务错误默认映射为 500
    pub fn internal(msg: impl Into<String>) -> Self {
        ApiError {
            status: 500,
            message: msg.into(),
            kind: None,
        }
    }

    pub fn with_kind(status: u16, msg: impl Into<String>, kind: &str) -> Self {
        ApiError {
            status,
            message: msg.into(),
            kind: Some(kind.to_string()),
        }
    }

    /// std::io::Error → 最近似的状态映射
    pub fn from_io(err: &std::io::Error) -> Self {
        let code = err.raw_os_error();
        let msg = || {
            // 优先给出原始 message
            err.to_string()
        };
        match err.kind() {
            std::io::ErrorKind::NotFound => ApiError::not_found(msg()),
            std::io::ErrorKind::PermissionDenied => ApiError::bad_request(msg()),
            std::io::ErrorKind::InvalidInput | std::io::ErrorKind::InvalidFilename => {
                ApiError::bad_request(msg())
            }
            _ => {
                // Windows 共享冲突 / 锁定
                if let Some(c) = code {
                    if c == 32 || c == 33 || c == 5 {
                        return ApiError::bad_request(msg());
                    }
                }
                ApiError::internal(msg())
            }
        }
    }

    pub fn from_io_msg(err: &std::io::Error, msg: impl Into<String>) -> Self {
        let mut e = ApiError::from_io(err);
        e.message = msg.into();
        e
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = StatusCode::from_u16(self.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        let mut body = json!({ "error": self.message });
        if let Some(kind) = self.kind {
            body["kind"] = json!(kind);
        }
        (status, Json(body)).into_response()
    }
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.message, self.status)
    }
}

impl std::error::Error for ApiError {}

pub type ApiResult<T> = Result<T, ApiError>;
