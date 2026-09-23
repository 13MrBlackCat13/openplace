//! Port of src/routes/leaderboard.ts — all endpoints are public (no auth).
use std::collections::HashMap;

use axum::extract::{Path, Query, State};
use axum::routing::get;
use axum::{Json, Router};
use serde_json::Value;

use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/leaderboard/region/{mode}/{country}",
            get(region_leaderboard),
        )
        .route("/leaderboard/country/{mode}", get(country_leaderboard))
        .route("/leaderboard/player/{mode}", get(player_leaderboard))
        .route("/leaderboard/alliance/{mode}", get(alliance_leaderboard))
        .route(
            "/leaderboard/region/players/{city}/{mode}",
            get(region_players),
        )
        .route(
            "/leaderboard/region/alliances/{city}/{mode}",
            get(region_alliances),
        )
}

fn is_valid_mode(mode: &str) -> bool {
    matches!(mode, "today" | "week" | "month" | "all-time")
}

/// GET /leaderboard/region/{mode}/{country}?limit=
async fn region_leaderboard(
    State(state): State<AppState>,
    Path((mode, country)): Path<(String, String)>,
    Query(params): Query<HashMap<String, String>>,
) -> ApiResult<Json<Value>> {
    if !is_valid_mode(&mode) {
        return Err(ApiError::bad_request("Invalid mode"));
    }
    // JS: parseInt garbage → NaN → treated as "no country".
    let country_id: i32 = country.parse().unwrap_or(0);
    let limit = params
        .get("limit")
        .and_then(|v| v.parse::<i64>().ok())
        .map(|v| v.clamp(1, 50))
        .unwrap_or(50);
    let entity_id = (country_id > 0).then_some(country_id);
    let response = state
        .leaderboard
        .get("region", &mode, entity_id, limit)
        .await?;
    Ok(Json(response))
}

/// GET /leaderboard/country/{mode}
async fn country_leaderboard(
    State(state): State<AppState>,
    Path(mode): Path<String>,
) -> ApiResult<Json<Value>> {
    if !is_valid_mode(&mode) {
        return Err(ApiError::bad_request("Invalid mode"));
    }
    let response = state.leaderboard.get("country", &mode, None, 50).await?;
    Ok(Json(response))
}

/// GET /leaderboard/player/{mode}
async fn player_leaderboard(
    State(state): State<AppState>,
    Path(mode): Path<String>,
) -> ApiResult<Json<Value>> {
    if !is_valid_mode(&mode) {
        return Err(ApiError::bad_request("Invalid mode"));
    }
    let response = state.leaderboard.get("player", &mode, None, 50).await?;
    Ok(Json(response))
}

/// GET /leaderboard/alliance/{mode}
async fn alliance_leaderboard(
    State(state): State<AppState>,
    Path(mode): Path<String>,
) -> ApiResult<Json<Value>> {
    if !is_valid_mode(&mode) {
        return Err(ApiError::bad_request("Invalid mode"));
    }
    let response = state.leaderboard.get("alliance", &mode, None, 50).await?;
    Ok(Json(response))
}

/// GET /leaderboard/region/players/{city}/{mode} — realtime region players.
/// JS checks mode and city together and answers 400 "Invalid params".
async fn region_players(
    State(state): State<AppState>,
    Path((city, mode)): Path<(String, String)>,
) -> ApiResult<Json<Value>> {
    let param_id = city.parse::<i32>().ok();
    if !is_valid_mode(&mode) || param_id.is_none() {
        return Err(ApiError::bad_request("Invalid params"));
    }
    // Accept a city_id (JS looked the region up by id; the port resolves via
    // city_id — for a city_id the mapped value is the same).
    let mut city_id = param_id.unwrap();
    if let Ok(Some(region)) = state.regions.by_city_id(city_id).await {
        city_id = region.city_id;
    }
    let response = state
        .leaderboard
        .get("regionPlayers", &mode, Some(city_id), 50)
        .await?;
    Ok(Json(response))
}

/// GET /leaderboard/region/alliances/{city}/{mode} — realtime region
/// alliances. JS validates mode ("Invalid mode") and city ("Invalid params")
/// separately here.
async fn region_alliances(
    State(state): State<AppState>,
    Path((city, mode)): Path<(String, String)>,
) -> ApiResult<Json<Value>> {
    if !is_valid_mode(&mode) {
        return Err(ApiError::bad_request("Invalid mode"));
    }
    let param_id = city
        .parse::<i32>()
        .map_err(|_| ApiError::bad_request("Invalid params"))?;
    let mut city_id = param_id;
    if let Ok(Some(region)) = state.regions.by_city_id(city_id).await {
        city_id = region.city_id;
    }
    let response = state
        .leaderboard
        .get("regionAlliances", &mode, Some(city_id), 50)
        .await?;
    Ok(Json(response))
}
