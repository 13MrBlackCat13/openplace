use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::json;

use crate::auth::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/health", get(health))
        .route("/checkrobots", get(checkrobots))
        .route("/challenge", get(challenge))
        .route("/payment/create-checkout-session", post(not_implemented))
        .route("/v1/autocomplete", get(autocomplete))
        .route("/favorite-location", post(create_favorite))
        .route("/favorite-location/delete", post(delete_favorite))
}

async fn health() -> Json<serde_json::Value> {
    Json(json!({}))
}

async fn checkrobots(State(state): State<AppState>) -> Json<serde_json::Value> {
    // Runtime-editable rules (admin panel) — read-lock snapshot, no cache.
    let s = state.settings_snapshot();
    Json(json!({
        "isMultiAccountAllowed": s.allow_multi_account,
        "isOffensiveContentAllowed": s.allow_offensive_content,
        "isExplicitContentAllowed": s.allow_explicit_content,
        "isGriefingAllowed": s.allow_griefing,
        "isKindGriefingAllowed": s.allow_kind_griefing,
        "isPoliticalGriefingAllowed": s.allow_political_griefing,
        "isVPNAllowed": s.allow_vpn,
        "isBottingAllowed": s.allow_bots,
        "extraRules": s.extra_rules,
    }))
}

async fn challenge(Query(params): Query<std::collections::HashMap<String, String>>) -> Redirect {
    let target = params
        .get("r")
        .map(|r| {
            if r.starts_with('/') {
                r.clone()
            } else {
                "/".to_string()
            }
        })
        .unwrap_or_else(|| "/".to_string());
    Redirect::to(&target)
}

async fn not_implemented() -> impl IntoResponse {
    (StatusCode::NOT_FOUND, "Payments are not implemented")
}

/// GET /v1/autocomplete?text= — GeoJSON FeatureCollection of regions.
async fn autocomplete(
    State(state): State<AppState>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> ApiResult<Json<serde_json::Value>> {
    let text = params.get("text").cloned().unwrap_or_default();
    if text.is_empty() {
        return Err(ApiError::bad_request("No query provided"));
    }
    let regions = state.regions.search(&text).await?;
    let features: Vec<serde_json::Value> = regions
        .iter()
        .map(|r| {
            json!({
                "type": "Feature",
                "geometry": { "type": "Point", "coordinates": [r.longitude, r.latitude] },
                "properties": {
                    "id": r.city_id,
                    "name": r.name,
                    "label": format!("{}, {}", r.name, country_name(r.country_id)),
                },
                "bbox": [r.longitude - 0.05, r.latitude - 0.05, r.longitude + 0.05, r.latitude + 0.05],
            })
        })
        .collect();
    Ok(Json(
        json!({ "type": "FeatureCollection", "features": features }),
    ))
}

fn country_name(country_id: i32) -> &'static str {
    crate::utils::country::COUNTRIES
        .iter()
        .find(|c| c.id == country_id)
        .map(|c| c.name)
        .unwrap_or("Unknown")
}

const MAX_FAVORITE_LOCATIONS: i64 = 50;

/// POST /favorite-location {latitude, longitude, name?}
async fn create_favorite(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<serde_json::Value>,
) -> ApiResult<Json<serde_json::Value>> {
    let latitude = body
        .get("latitude")
        .and_then(|v| v.as_f64())
        .ok_or_else(|| ApiError::bad_request("Invalid coordinates"))?;
    let longitude = body
        .get("longitude")
        .and_then(|v| v.as_f64())
        .ok_or_else(|| ApiError::bad_request("Invalid coordinates"))?;
    let name = body
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let exists: Option<(i32,)> = sqlx::query_as("SELECT id FROM users WHERE id = $1")
        .bind(auth.id)
        .fetch_optional(&state.pool)
        .await?;
    if exists.is_none() {
        return Err(ApiError::UserNotFound);
    }
    let (count,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM favorite_locations WHERE user_id = $1")
            .bind(auth.id)
            .fetch_one(&state.pool)
            .await?;
    if count >= MAX_FAVORITE_LOCATIONS {
        return Err(ApiError::forbidden("Maximum favorite locations reached"));
    }
    let (id,): (i32,) = sqlx::query_as(
        "INSERT INTO favorite_locations (user_id, name, latitude, longitude) \
         VALUES ($1, $2, $3, $4) RETURNING id",
    )
    .bind(auth.id)
    .bind(name)
    .bind(latitude)
    .bind(longitude)
    .fetch_one(&state.pool)
    .await?;
    Ok(Json(json!({ "id": id, "success": true })))
}

/// POST /favorite-location/delete {id}
async fn delete_favorite(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<serde_json::Value>,
) -> ApiResult<Json<serde_json::Value>> {
    let id = body
        .get("id")
        .and_then(|v| v.as_i64())
        .map(|v| v as i32)
        .ok_or_else(|| ApiError::bad_request("Invalid location ID"))?;
    let deleted = sqlx::query("DELETE FROM favorite_locations WHERE id = $1 AND user_id = $2")
        .bind(id)
        .bind(auth.id)
        .execute(&state.pool)
        .await?
        .rows_affected();
    if deleted == 0 {
        return Err(ApiError::bad_request("Favorite location not found"));
    }
    Ok(Json(json!({ "success": true })))
}
