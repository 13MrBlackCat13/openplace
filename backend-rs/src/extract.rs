use axum::extract::FromRequest;
use serde::de::DeserializeOwned;

use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

/// JSON body extractor that ignores Content-Type — the JS backend parsed
/// `application/json` AND `text/plain` (the wplace frontend sends text/plain),
/// and returned 400 "Invalid JSON format" on parse errors.
///
/// Use this instead of `axum::Json` for all request bodies.
pub struct FlexibleJson<T>(pub T);

impl<T> FlexibleJson<T> {
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T> std::ops::Deref for FlexibleJson<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T: DeserializeOwned> FromRequest<AppState> for FlexibleJson<T> {
    type Rejection = ApiError;

    async fn from_request(req: axum::extract::Request, _state: &AppState) -> ApiResult<Self> {
        let limited = req.into_body();
        let bytes = axum::body::to_bytes(limited, 64 * 1024 * 1024)
            .await
            .map_err(|_| ApiError::bad_request("Invalid JSON format"))?;
        if bytes.is_empty() {
            return Err(ApiError::bad_request("Invalid JSON format"));
        }
        serde_json::from_slice(&bytes)
            .map(FlexibleJson)
            .map_err(|_| ApiError::bad_request("Invalid JSON format"))
    }
}

/// Client IP resolution mirroring the JS middleware:
/// cf-connecting-ip → x-forwarded-for (first entry) → socket address.
pub fn client_ip(parts: &axum::http::request::Parts) -> String {
    if let Some(v) = parts
        .headers
        .get("cf-connecting-ip")
        .and_then(|v| v.to_str().ok())
    {
        return normalize_ip(v);
    }
    if let Some(v) = parts
        .headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
    {
        let first = v.split(',').next().unwrap_or(v).trim();
        return normalize_ip(first);
    }
    if let Some(addr) = parts
        .extensions
        .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>()
    {
        return addr.ip().to_string();
    }
    "127.0.0.1".to_string()
}

fn normalize_ip(value: &str) -> String {
    let value = value.trim();
    if value.contains(':') {
        // More than one ':' → IPv6 literal (keep as-is), otherwise IPv4:port.
        if value.matches(':').count() > 1 {
            return value.to_string();
        }
        return value.split(':').next().unwrap_or(value).to_string();
    }
    if value.len() >= 7 && (value.contains('.') || value.contains(':')) {
        value.to_string()
    } else {
        "127.0.0.1".to_string()
    }
}
