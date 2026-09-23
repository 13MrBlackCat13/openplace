use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

/// Error type mirroring the JS backend's `handleServiceError` mapping
/// (src/middleware/errorHandler.ts) — message → status/body contract is
/// API-compatible, including the 451 suspension format and the 208 code.
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("{0}")]
    BadRequest(String),
    #[error("Unauthorized")]
    Unauthorized,
    /// 401 with body `{"error":"refresh","status":401}` — client must re-login.
    #[error("refresh")]
    Refresh,
    #[error("{0}")]
    Forbidden(String),
    #[error("User not found")]
    UserNotFound,
    #[error("No Alliance")]
    NoAlliance,
    #[error("Not Found")]
    NotFound,
    #[error("Already Reported")]
    AlreadyReported,
    /// 451 `{ "err": ..., "suspension": "ban" | "timeout" }`
    #[error("suspended")]
    Suspension { reason: String, kind: &'static str },
    /// 403 for paint-specific failures (keeps JS message text).
    #[error("{0}")]
    PaintForbidden(String),
    #[error("Internal Server Error")]
    Internal,
}

impl ApiError {
    pub fn bad_request(msg: impl Into<String>) -> Self {
        Self::BadRequest(msg.into())
    }
    pub fn forbidden(msg: impl Into<String>) -> Self {
        Self::Forbidden(msg.into())
    }
    pub fn ban(reason: Option<String>) -> Self {
        Self::Suspension {
            reason: reason.unwrap_or_else(|| "other".to_string()),
            kind: "ban",
        }
    }
    pub fn timeout(reason: Option<String>) -> Self {
        Self::Suspension {
            reason: reason.unwrap_or_else(|| "other".to_string()),
            kind: "timeout",
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, body) = match &self {
            ApiError::BadRequest(msg) => (
                StatusCode::BAD_REQUEST,
                json!({ "error": msg, "status": 400 }),
            ),
            ApiError::Unauthorized => (
                StatusCode::UNAUTHORIZED,
                json!({ "error": "Unauthorized", "status": 401 }),
            ),
            ApiError::Refresh => (
                StatusCode::UNAUTHORIZED,
                json!({ "error": "refresh", "status": 401 }),
            ),
            ApiError::Forbidden(msg) => (
                StatusCode::FORBIDDEN,
                json!({ "error": msg, "status": 403 }),
            ),
            ApiError::UserNotFound => (
                StatusCode::NOT_FOUND,
                json!({ "error": "User not found", "status": 404 }),
            ),
            ApiError::NoAlliance => (
                StatusCode::NOT_FOUND,
                json!({ "error": "No Alliance", "status": 404 }),
            ),
            ApiError::NotFound => (
                StatusCode::NOT_FOUND,
                json!({ "error": "Not Found", "status": 404 }),
            ),
            ApiError::AlreadyReported => (
                StatusCode::ALREADY_REPORTED,
                json!({ "error": "Already Reported", "status": 208 }),
            ),
            ApiError::Suspension { reason, kind } => (
                StatusCode::from_u16(451).unwrap(),
                json!({ "err": reason, "suspension": kind }),
            ),
            ApiError::PaintForbidden(msg) => (
                StatusCode::FORBIDDEN,
                json!({ "error": msg, "status": 403 }),
            ),
            ApiError::Internal => (
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({ "error": "Internal Server Error", "status": 500 }),
            ),
        };
        (status, Json(body)).into_response()
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(err: sqlx::Error) -> Self {
        tracing_or_log(&err.to_string());
        ApiError::Internal
    }
}

impl From<std::io::Error> for ApiError {
    fn from(err: std::io::Error) -> Self {
        tracing_or_log(&err.to_string());
        ApiError::Internal
    }
}

impl From<serde_json::Error> for ApiError {
    fn from(err: serde_json::Error) -> Self {
        tracing_or_log(&err.to_string());
        ApiError::Internal
    }
}

// The crate does not pull in `tracing`; route/service code logs through eprintln
// (same visibility as the JS backend's console output).
fn tracing_or_log(msg: &str) {
    eprintln!("[error] {msg}");
}

pub type ApiResult<T> = Result<T, ApiError>;
