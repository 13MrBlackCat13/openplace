//! Notification endpoints — port of src/routes/notification.ts. All storage
//! logic lives in crate::services::notification::NotificationService.

use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{json, Value};

use crate::auth::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::extract::FlexibleJson;
use crate::services::notification::NotificationService;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/notification/count", get(unread_count))
        .route("/notification/page", get(notification_page))
        .route("/notification/mark-read", post(mark_read))
        .route("/notification/mark-read/all", post(mark_all_read))
}

/// GET /notification/count
async fn unread_count(State(state): State<AppState>, auth: AuthUser) -> ApiResult<Json<Value>> {
    let count = NotificationService::unread_count(&state.pool, auth.id).await?;
    Ok(Json(json!({ "count": count })))
}

/// GET /notification/page — limit 10, as in the JS service default.
async fn notification_page(
    State(state): State<AppState>,
    auth: AuthUser,
) -> ApiResult<Json<Value>> {
    NotificationService::page(&state.pool, auth.id, 10).await
}

/// POST /notification/mark-read {notificationIds: number[]}
async fn mark_read(
    State(state): State<AppState>,
    auth: AuthUser,
    FlexibleJson(body): FlexibleJson<Value>,
) -> ApiResult<Json<Value>> {
    let Some(ids) = body.get("notificationIds").and_then(Value::as_array) else {
        return Err(ApiError::bad_request("notificationIds must be an array"));
    };
    let mut parsed: Vec<i64> = Vec::with_capacity(ids.len());
    for id in ids {
        let Some(v) = id.as_i64() else {
            return Err(ApiError::bad_request("Invalid notification IDs"));
        };
        parsed.push(v);
    }
    NotificationService::mark_read(&state.pool, auth.id, &parsed).await?;
    Ok(Json(json!({ "success": true })))
}

/// POST /notification/mark-read/all
async fn mark_all_read(State(state): State<AppState>, auth: AuthUser) -> ApiResult<Json<Value>> {
    NotificationService::mark_all_read(&state.pool, auth.id).await?;
    Ok(Json(json!({ "success": true })))
}
