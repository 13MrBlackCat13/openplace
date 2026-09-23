// Pixel endpoints — port of src/routes/pixel.ts (the hottest API surface):
//   GET  /{season}/tile/random
//   GET  /{season}/pixel/{tile_x}/{tile_y}?x=&y=
//   GET  /files/{season}/tiles/{tile_x}/{tile_y}.png
//   POST /{season}/pixel/{tile_x}/{tile_y}   (auth + paint service)
use std::collections::HashMap;
use std::time::{Duration, UNIX_EPOCH};

use axum::extract::{Path, Query, State};
use axum::http::request::Parts;
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use chrono::{SecondsFormat, Utc};
use serde_json::{json, Value};

use crate::auth::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::extract::{client_ip, FlexibleJson};
use crate::services::paint::{paint_pixels, PaintInput};
use crate::state::AppState;
use crate::utils::colors::TILE_SIZE;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/{season}/tile/random", get(random_tile))
        .route(
            "/{season}/pixel/{tile_x}/{tile_y}",
            get(pixel_info).post(paint_pixel),
        )
        // axum 0.8 has no suffix parameters, so the "{tile_y}.png" segment is
        // captured as {file} and split below.
        .route("/files/{season}/tiles/{tile_x}/{file}", get(tile_png))
}

// ---------------------------------------------------------------------------
// GET /{season}/tile/random
// ---------------------------------------------------------------------------

async fn random_tile(
    State(state): State<AppState>,
    Path(season): Path<String>,
) -> ApiResult<Json<Value>> {
    if season != "s0" {
        return Err(ApiError::bad_request("Bad Request"));
    }
    // JS loops random ids until a hit; one indexed seek per attempt is enough.
    for _ in 0..5 {
        let row: Option<(i32, i32, i16, i16)> = sqlx::query_as(
            "SELECT tile_x, tile_y, x, y FROM pixels \
             WHERE season = 0 \
               AND id >= (SELECT floor(random() * GREATEST( \
                              (SELECT max(id) FROM pixels WHERE season = 0), 1)) + 1) \
             ORDER BY id LIMIT 1",
        )
        .fetch_optional(&state.pool)
        .await?;
        if let Some((tile_x, tile_y, x, y)) = row {
            return Ok(Json(json!({
                "pixel": { "x": x, "y": y },
                "tile": { "x": tile_x, "y": tile_y },
            })));
        }
    }
    // Nothing painted yet (or 5 unlucky seeks on a sparse table).
    Ok(Json(json!({
        "pixel": { "x": 500, "y": 500 },
        "tile": { "x": 1024, "y": 1024 },
    })))
}

// ---------------------------------------------------------------------------
// GET /{season}/pixel/{tile_x}/{tile_y}?x=&y=
// ---------------------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct PixelAuthorRow {
    painted_by: i32,
    painted_at: chrono::DateTime<chrono::Utc>,
    banned: bool,
    nickname: Option<String>,
    name: String,
    equipped_flag: i32,
    picture: Option<String>,
    discord: Option<String>,
    discord_user_id: Option<String>,
    verified: bool,
    alliance_id: Option<i32>,
    alliance_name: Option<String>,
}

async fn pixel_info(
    State(state): State<AppState>,
    Path((season, tile_x, tile_y)): Path<(String, i32, i32)>,
    Query(params): Query<HashMap<String, String>>,
) -> ApiResult<Json<Value>> {
    if season != "s0" {
        return Err(ApiError::bad_request("Bad Request"));
    }
    let Some(x) = params.get("x").and_then(|v| v.parse::<i32>().ok()) else {
        return Err(ApiError::bad_request("Bad Request"));
    };
    let Some(y) = params.get("y").and_then(|v| v.parse::<i32>().ok()) else {
        return Err(ApiError::bad_request("Bad Request"));
    };
    if !(0..TILE_SIZE).contains(&x) || !(0..TILE_SIZE).contains(&y) {
        return Err(ApiError::bad_request("Bad Request"));
    }

    state.regions.ensure_loaded().await;
    let region = state.regions.for_pixel(tile_x, tile_y, x, y);

    let author: Option<PixelAuthorRow> = sqlx::query_as(
        "SELECT p.painted_by, p.painted_at, u.banned, u.nickname, u.name, u.equipped_flag, \
                u.picture, u.discord, u.discord_user_id, u.verified, \
                a.id AS alliance_id, a.name AS alliance_name \
         FROM pixels p \
         JOIN users u ON u.id = p.painted_by \
         LEFT JOIN alliances a ON a.id = u.alliance_id \
         WHERE p.season = 0 AND p.tile_x = $1 AND p.tile_y = $2 AND p.x = $3 AND p.y = $4",
    )
    .bind(tile_x)
    .bind(tile_y)
    .bind(x as i16)
    .bind(y as i16)
    .fetch_optional(&state.pool)
    .await?;

    let painted_by = match author {
        None => json!({ "id": 0 }),
        Some(a) => {
            let painted_at = a.painted_at.to_rfc3339_opts(SecondsFormat::Millis, true);
            if a.banned {
                json!({
                    "id": -1,
                    "name": "Suspended Account",
                    "paintedAt": painted_at,
                })
            } else {
                json!({
                    "id": a.painted_by,
                    "name": a.nickname.filter(|n| !n.is_empty()).unwrap_or(a.name),
                    "allianceId": a.alliance_id.unwrap_or(0),
                    "allianceName": a.alliance_name.unwrap_or_default(),
                    "equippedFlag": a.equipped_flag,
                    "picture": a.picture.unwrap_or_default(),
                    "discord": a.discord.unwrap_or_default(),
                    "discordUserId": a.discord_user_id.unwrap_or_default(),
                    "verified": a.verified,
                    "paintedAt": painted_at,
                })
            }
        }
    };

    Ok(Json(json!({
        "region": {
            "id": region.id,
            "cityId": region.city_id,
            "name": region.name,
            "number": region.number,
            "countryId": region.country_id,
            "flagId": region.country_id,
        },
        "paintedBy": painted_by,
    })))
}

// ---------------------------------------------------------------------------
// GET /files/{season}/tiles/{tile_x}/{tile_y}.png
// ---------------------------------------------------------------------------

async fn tile_png(
    State(state): State<AppState>,
    Path((season, tile_x_raw, file)): Path<(String, String, String)>,
    parts: Parts,
) -> Response {
    if season != "s0" {
        return ApiError::bad_request("Bad Request").into_response();
    }
    let Ok(tile_x) = tile_x_raw.parse::<i32>() else {
        return ApiError::bad_request("Bad Request").into_response();
    };
    let Some(tile_y_raw) = file.strip_suffix(".png") else {
        return ApiError::bad_request("Bad Request").into_response();
    };
    let Ok(tile_y) = tile_y_raw.parse::<i32>() else {
        return ApiError::bad_request("Bad Request").into_response();
    };

    let entry = match state.tiles.entry(&state.pool, tile_x, tile_y).await {
        Ok(entry) => entry,
        Err(err) => return ApiError::from(err).into_response(),
    };
    let (png, ts) = state.tiles.serve(&entry);

    let last_modified = httpdate::fmt_http_date(UNIX_EPOCH + Duration::from_secs(ts.max(0) as u64));

    // Conditional GET — JS: floor(updatedAt / 1000) <= floor(ifModifiedSince / 1000).
    if let Some(ims) = parts
        .headers
        .get(header::IF_MODIFIED_SINCE)
        .and_then(|v| v.to_str().ok())
    {
        if let Ok(parsed) = httpdate::parse_http_date(ims) {
            let parsed_secs = parsed
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            if ts <= parsed_secs {
                return tile_response(StatusCode::NOT_MODIFIED, &last_modified, None);
            }
        }
    }

    tile_response(StatusCode::OK, &last_modified, Some((*png).clone()))
}

fn tile_response(status: StatusCode, last_modified: &str, body: Option<Vec<u8>>) -> Response {
    let builder = Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "image/png")
        .header(header::LAST_MODIFIED, last_modified)
        .header(header::CACHE_CONTROL, "private, must-revalidate")
        .header(header::PRAGMA, "no-cache")
        .header(header::EXPIRES, "0");
    let body = match body {
        Some(bytes) => axum::body::Body::from(bytes),
        None => axum::body::Body::empty(),
    };
    builder.body(body).expect("static response headers")
}

// ---------------------------------------------------------------------------
// POST /{season}/pixel/{tile_x}/{tile_y}
// ---------------------------------------------------------------------------

async fn paint_pixel(
    parts: Parts,
    State(state): State<AppState>,
    Path((season, tile_x, tile_y)): Path<(String, i32, i32)>,
    auth: AuthUser,
    FlexibleJson(body): FlexibleJson<Value>,
) -> Response {
    let ip = client_ip(&parts);

    // 429 — raw response, deliberately outside the ApiError body format.
    if state.config.enable_rate_limit
        && state
            .limiter
            .check(
                &ip,
                state.config.paint_rate_limit_attempts,
                state.config.paint_rate_limit_ms,
            )
            .is_some()
    {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            Json(json!({ "error": "Too many requests. Please slow down." })),
        )
            .into_response();
    }

    if season != "s0" {
        return ApiError::bad_request("Bad Request").into_response();
    }

    // Body: { colors: number[], coords: number[] } with len(colors) * 2 == len(coords).
    let (Ok(colors), Ok(coords)) = (
        parse_i64_array(&body, "colors"),
        parse_i64_array(&body, "coords"),
    ) else {
        return ApiError::bad_request("Bad Request").into_response();
    };
    if colors.len() * 2 != coords.len() {
        return ApiError::bad_request("Bad Request").into_response();
    }
    let pixel_count = colors.len();

    // Pre-check suspended accounts (the service repeats it — same double read
    // as the JS backend; safe to duplicate).
    if let Some(runtime) = state.users.get_or_load(&state.pool, auth.id).await {
        if runtime.banned {
            eprintln!(
                "[{}] [{ip}] {}#{} attempted to paint {} pixels at tile ({tile_x}, {tile_y}) while banned.",
                Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
                runtime.name,
                runtime.id,
                colors.len(),
            );
            return ApiError::ban(runtime.suspension_reason.clone()).into_response();
        }
        if runtime.timeout_until > Utc::now() {
            eprintln!(
                "[{}] [{ip}] {}#{} attempted to paint {} pixels at tile ({tile_x}, {tile_y}) while timed out.",
                Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
                runtime.name,
                runtime.id,
                colors.len(),
            );
            return ApiError::timeout(runtime.suspension_reason.clone()).into_response();
        }
    }

    // Anti-automation gate (blocks only in enforce mode, never for staff).
    if let Err(err) = crate::services::antibot::pre_check(&state, auth.id).await {
        return err.into_response();
    }

    let country_header = parts
        .headers
        .get("cf-ipcountry")
        .and_then(|v| v.to_str().ok());
    match paint_pixels(
        &state,
        &auth,
        &ip,
        country_header,
        tile_x,
        tile_y,
        PaintInput { colors, coords },
    )
    .await
    {
        Ok(Json(value)) => {
            crate::services::antibot::observe_paint(
                &state,
                auth.id,
                pixel_count,
                parts
                    .headers
                    .get("user-agent")
                    .and_then(|v| v.to_str().ok()),
            );
            Json(value).into_response()
        }
        Err(err) => err.into_response(),
    }
}

/// All elements must be JSON numbers; anything else (or a missing/non-array
/// field) is a Bad Request.
fn parse_i64_array(body: &Value, key: &str) -> Result<Vec<i64>, ()> {
    let items = body.get(key).and_then(Value::as_array).ok_or(())?;
    items.iter().map(|v| v.as_i64().ok_or(())).collect()
}
