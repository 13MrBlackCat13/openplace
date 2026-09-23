// Moderator endpoints — port of src/routes/moderator.ts. Everything requires
// auth + require_moderator, except the pixel redirect (no auth, like JS).
use std::collections::{HashMap, HashSet};

use axum::extract::{Path, Query, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::auth::{require_moderator, AuthUser};
use crate::error::{ApiError, ApiResult};
use crate::extract::FlexibleJson;
use crate::routes::report::js_parse_int;
use crate::services::ticket;
use crate::state::AppState;
use crate::utils::colors::TILE_SIZE;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/moderator/tickets", get(moderator_tickets))
        .route("/moderator/users", post(moderator_users))
        .route("/moderator/users/tickets", get(moderator_user_tickets))
        .route("/moderator/open-tickets-count", get(open_tickets_count))
        .route(
            "/moderator/severe-open-tickets-count",
            post(severe_open_tickets_count),
        )
        .route("/moderator/assign-new-tickets", post(assign_new_tickets))
        .route("/moderator/count-my-tickets", get(count_my_tickets))
        .route("/moderator/set-ticket-status", post(set_ticket_status))
        .route("/moderation", get(moderation_html))
        // No auth (JS route registers no middleware): 302 back to the
        // non-moderator pixel path.
        .route(
            "/moderator/{season}/pixel/{tile_x}/{tile_y}",
            get(pixel_redirect),
        )
        .route(
            "/moderator/pixel-area/{season}/{tile_x}/{tile_y}",
            get(pixel_area),
        )
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

fn iso_ms(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(SecondsFormat::Millis, true)
}

/// JS `nickname || name` — an empty-string nickname falls back to name.
fn display_name(nickname: &Option<String>, name: &str) -> String {
    nickname
        .as_ref()
        .filter(|n| !n.is_empty())
        .cloned()
        .unwrap_or_else(|| name.to_string())
}

/// JS `Number.parseInt(query) || fallback` — NaN/0 fall back (falsy).
fn parse_int_or(params: &HashMap<String, String>, key: &str, fallback: i64) -> i64 {
    match js_parse_int(params.get(key).map(String::as_str).unwrap_or("")) {
        Some(v) if v != 0.0 => v as i64,
        _ => fallback,
    }
}

// ---------------------------------------------------------------------------
// GET /moderator/tickets?page=&limit=
// ---------------------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct ModTicketRow {
    id: Uuid,
    user_id: Option<i32>,
    reported_user_id: Option<i32>,
    latitude: f64,
    longitude: f64,
    zoom: f64,
    reason: String,
    notes: String,
    image: Option<Vec<u8>>,
    created_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
struct ModUserRow {
    id: i32,
    name: String,
    nickname: Option<String>,
    discord: Option<String>,
    country: String,
    banned: bool,
    role: String,
    picture: Option<String>,
    pixels_painted: i32,
    last_ip: Option<String>,
    registration_ip: Option<String>,
}

async fn moderator_tickets(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(params): Query<HashMap<String, String>>,
) -> ApiResult<Json<Value>> {
    require_moderator(&state, auth.id).await?;

    let page = parse_int_or(&params, "page", 1);
    let limit = parse_int_or(&params, "limit", 20);
    if !(1..=10_000).contains(&page) || !(1..=100).contains(&limit) {
        return Err(ApiError::bad_request("Bad Request"));
    }
    let offset = (page - 1) * limit;

    let tickets: Vec<ModTicketRow> = sqlx::query_as(
        "SELECT id, user_id, reported_user_id, latitude, longitude, zoom, reason, notes, image, created_at \
         FROM tickets WHERE resolution IS NULL ORDER BY created_at DESC LIMIT $1 OFFSET $2",
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.pool)
    .await?;

    // Distinct involved users (authors + reported users).
    let mut ids: Vec<i32> = Vec::new();
    for t in &tickets {
        for uid in [t.user_id, t.reported_user_id].into_iter().flatten() {
            if !ids.contains(&uid) {
                ids.push(uid);
            }
        }
    }

    let users: HashMap<i32, ModUserRow> = if ids.is_empty() {
        HashMap::new()
    } else {
        let rows: Vec<ModUserRow> = sqlx::query_as(
            "SELECT id, name, nickname, discord, country, banned, role, picture, pixels_painted, \
                    last_ip, registration_ip \
             FROM users WHERE id = ANY($1)",
        )
        .bind(&ids)
        .fetch_all(&state.pool)
        .await?;
        rows.into_iter().map(|u| (u.id, u)).collect()
    };

    // reportedCount / timeoutCount per reported user (JS groupBy over tickets;
    // the JS "Timeout" literal never matches the lowercase enum — using the
    // effective 'timeout' value here).
    let mut reported_counts: HashMap<i32, i64> = HashMap::new();
    let mut timeout_counts: HashMap<i32, i64> = HashMap::new();
    let mut author_report_counts: HashMap<i32, i64> = HashMap::new();
    if !ids.is_empty() {
        let rows: Vec<(i32, i64, i64)> = sqlx::query_as(
            "SELECT reported_user_id, count(*), count(*) FILTER (WHERE resolution = 'timeout') \
             FROM tickets WHERE reported_user_id = ANY($1) GROUP BY reported_user_id",
        )
        .bind(&ids)
        .fetch_all(&state.pool)
        .await?;
        for (rid, total, timeouts) in rows {
            reported_counts.insert(rid, total);
            timeout_counts.insert(rid, timeouts);
        }
        let rows: Vec<(i32, i64)> = sqlx::query_as(
            "SELECT user_id, count(*) FROM tickets WHERE user_id = ANY($1) GROUP BY user_id",
        )
        .bind(&ids)
        .fetch_all(&state.pool)
        .await?;
        for (uid, total) in rows {
            author_report_counts.insert(uid, total);
        }
    }

    // sameIpAccounts: other users (outside the involved set) sharing any of
    // the involved user's last_ip/registration_ip.
    let mut user_ips: HashMap<i32, Vec<String>> = HashMap::new();
    let mut all_ips: Vec<String> = Vec::new();
    let mut seen_ips: HashSet<String> = HashSet::new();
    for u in users.values() {
        let mut ips = Vec::new();
        for ip in [u.last_ip.as_deref(), u.registration_ip.as_deref()]
            .into_iter()
            .flatten()
        {
            ips.push(ip.to_string());
            if seen_ips.insert(ip.to_string()) {
                all_ips.push(ip.to_string());
            }
        }
        user_ips.insert(u.id, ips);
    }
    let others: Vec<(i32, Option<String>, Option<String>)> = if all_ips.is_empty() {
        Vec::new()
    } else {
        sqlx::query_as(
            "SELECT id, last_ip, registration_ip FROM users \
             WHERE (last_ip = ANY($1) OR registration_ip = ANY($1)) AND NOT (id = ANY($2))",
        )
        .bind(&all_ips)
        .bind(&ids)
        .fetch_all(&state.pool)
        .await?
    };
    let mut same_ip_counts: HashMap<i32, i64> = HashMap::new();
    for u in users.values() {
        let ips: &[String] = user_ips.get(&u.id).map(Vec::as_slice).unwrap_or(&[]);
        let count = others
            .iter()
            .filter(|(_, last, reg)| {
                last.as_deref()
                    .is_some_and(|ip| ips.iter().any(|mine| mine == ip))
                    || reg
                        .as_deref()
                        .is_some_and(|ip| ips.iter().any(|mine| mine == ip))
            })
            .count() as i64;
        same_ip_counts.insert(u.id, count);
    }

    let mut out = Vec::with_capacity(tickets.len());
    for t in &tickets {
        let author = t.user_id.and_then(|id| users.get(&id));
        let reported = t.reported_user_id.and_then(|id| users.get(&id));

        let reported_id = reported.map(|r| r.id).unwrap_or(0);
        let reported_count = reported_counts.get(&reported_id).copied().unwrap_or(0);
        let timeout_count = timeout_counts.get(&reported_id).copied().unwrap_or(0);
        let pixels = reported.map(|r| r.pixels_painted).unwrap_or(0);
        let same_ip = same_ip_counts.get(&reported_id).copied().unwrap_or(0);

        let author_id = author.map(|a| a.id).unwrap_or(0);
        let author_report_count = author_report_counts.get(&author_id).copied().unwrap_or(0);
        let author_pixels = author.map(|a| a.pixels_painted).unwrap_or(0);

        let author_json = author.map(|a| {
            json!({
                "userId": a.id,
                "name": display_name(&a.nickname, &a.name),
                "discord": a.discord,
                "country": a.country,
                "banned": a.banned,
                "role": a.role,
                "reportedCount": author_report_count,
                "pixelsPainted": author_pixels,
            })
        });

        let reported_json = reported.map(|r| {
            json!({
                "userId": r.id,
                "id": r.id,
                "name": display_name(&r.nickname, &r.name),
                "discord": r.discord,
                "country": r.country,
                "banned": r.banned,
                "role": r.role,
                "picture": r.picture.clone().unwrap_or_default(),
                "reportedCount": reported_count,
                "timeoutCount": timeout_count,
                "pixelsPainted": pixels,
                "lastTimeoutReason": Value::Null,
            })
        });

        let report = json!({
            "id": t.id.to_string(),
            "reportedLatitude": t.latitude,
            "reportedLongitude": t.longitude,
            "zoom": t.zoom,
            "reason": t.reason,
            "notes": t.notes,
            "imageUrl": t.image.as_ref()
                .map(|b| format!("data:image/jpeg;base64,{}", BASE64.encode(b)))
                .unwrap_or_default(),
            "createdAt": iso_ms(t.created_at),
            "userId": reported_id,
            "reportedByName": author.as_ref()
                .map(|a| display_name(&a.nickname, &a.name))
                .unwrap_or_default(),
            "reportedByPicture": author.and_then(|a| a.picture.clone()).unwrap_or_default(),
            "reportedBy": author_id,
            "reportedCount": reported_count,
            "timeoutCount": timeout_count,
            "lastTimeoutReason": Value::Null,
            "sameIpAccounts": same_ip,
            "pixelsPainted": pixels,
            "allianceId": 0,
            "allianceName": "",
        });

        out.push(json!({
            "id": t.id.to_string(),
            "author": author_json,
            "reportedUser": reported_json,
            "createdAt": iso_ms(t.created_at),
            "reports": [report],
        }));
    }

    let (total,): (i64,) = sqlx::query_as("SELECT count(*) FROM tickets WHERE resolution IS NULL")
        .fetch_one(&state.pool)
        .await?;
    let total_pages = total / limit + i64::from(total % limit != 0);

    Ok(Json(json!({
        "tickets": out,
        "pagination": {
            "page": page,
            "limit": limit,
            "total": total,
            "totalPages": total_pages,
        },
        "status": 200,
    })))
}

// ---------------------------------------------------------------------------
// POST /moderator/users {userIds: number[]}
// ---------------------------------------------------------------------------

async fn moderator_users(
    State(state): State<AppState>,
    auth: AuthUser,
    FlexibleJson(body): FlexibleJson<Value>,
) -> ApiResult<Json<Value>> {
    require_moderator(&state, auth.id).await?;

    // JS filter: drop non-numbers, NaN and anything <= 0.
    let ids: Vec<i32> = body
        .get("userIds")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|v| {
                    v.as_i64()
                        .filter(|n| *n > 0)
                        .and_then(|n| i32::try_from(n).ok())
                })
                .collect()
        })
        .unwrap_or_default();

    let rows: Vec<(i32, String, Option<String>, bool, Option<String>)> = if ids.is_empty() {
        Vec::new()
    } else {
        sqlx::query_as("SELECT id, name, nickname, banned, picture FROM users WHERE id = ANY($1)")
            .bind(&ids)
            .fetch_all(&state.pool)
            .await?
    };

    let users: Vec<Value> = rows
        .iter()
        .map(|(id, name, nickname, banned, picture)| {
            json!({
                "userId": id,
                "id": id,
                "name": display_name(nickname, name),
                "banned": banned,
                "picture": picture.clone().unwrap_or_default(),
            })
        })
        .collect();
    Ok(Json(json!({ "users": users })))
}

// ---------------------------------------------------------------------------
// GET /moderator/users/tickets?userId=
// ---------------------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct AuthorRow {
    id: i32,
    name: String,
    nickname: Option<String>,
    discord: Option<String>,
    country: String,
    banned: bool,
}

async fn moderator_user_tickets(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(params): Query<HashMap<String, String>>,
) -> ApiResult<Json<Value>> {
    require_moderator(&state, auth.id).await?;

    let user_id = match js_parse_int(params.get("userId").map(String::as_str).unwrap_or("")) {
        Some(v) if v > 0.0 => v as i64,
        _ => return Err(ApiError::bad_request("Bad Request")),
    };
    let Ok(user_id) = i32::try_from(user_id) else {
        return Err(ApiError::UserNotFound);
    };
    let Some((rid, rname, rnickname, rdiscord, rcountry, rbanned)): Option<(
        i32,
        String,
        Option<String>,
        Option<String>,
        String,
        bool,
    )> = sqlx::query_as(
        "SELECT id, name, nickname, discord, country, banned FROM users WHERE id = $1",
    )
    .bind(user_id)
    .fetch_optional(&state.pool)
    .await?
    else {
        return Err(ApiError::UserNotFound);
    };
    let reported_json = json!({
        "id": rid,
        "name": display_name(&rnickname, &rname),
        "discord": rdiscord.unwrap_or_default(),
        "country": rcountry,
        "banned": rbanned,
    });

    let tickets: Vec<ModTicketRow> = sqlx::query_as(
        "SELECT id, user_id, reported_user_id, latitude, longitude, zoom, reason, notes, image, created_at \
         FROM tickets WHERE reported_user_id = $1 ORDER BY created_at",
    )
    .bind(user_id)
    .fetch_all(&state.pool)
    .await?;

    let mut author_ids: Vec<i32> = Vec::new();
    for t in &tickets {
        if let Some(aid) = t.user_id {
            if !author_ids.contains(&aid) {
                author_ids.push(aid);
            }
        }
    }
    let authors: HashMap<i32, AuthorRow> = if author_ids.is_empty() {
        HashMap::new()
    } else {
        let rows: Vec<AuthorRow> = sqlx::query_as(
            "SELECT id, name, nickname, discord, country, banned FROM users WHERE id = ANY($1)",
        )
        .bind(&author_ids)
        .fetch_all(&state.pool)
        .await?;
        rows.into_iter().map(|a| (a.id, a)).collect()
    };

    // JS quirk kept: the outer id is the reported user id, not the ticket id.
    let tickets_json: Vec<Value> = tickets
        .iter()
        .map(|t| {
            let author_json = t.user_id.and_then(|id| authors.get(&id)).map(|a| {
                json!({
                    "id": a.id,
                    "name": display_name(&a.nickname, &a.name),
                    "discord": a.discord.clone().unwrap_or_default(),
                    "country": a.country,
                    "banned": a.banned,
                })
            });
            json!({
                "id": rid,
                "author": author_json,
                "reportedUser": reported_json,
                "createdAt": iso_ms(t.created_at),
                "reports": [{
                    "id": t.id.to_string(),
                    "latitude": t.latitude,
                    "longitude": t.longitude,
                    "zoom": t.zoom,
                    "reason": t.reason,
                    "notes": t.notes,
                    "image": t.image.as_ref().map(|b| BASE64.encode(b)).unwrap_or_default(),
                    "createdAt": iso_ms(t.created_at),
                }],
            })
        })
        .collect();

    Ok(Json(json!({ "tickets": tickets_json, "status": 200 })))
}

// ---------------------------------------------------------------------------
// Counters / stubs
// ---------------------------------------------------------------------------

async fn open_tickets_count(
    State(state): State<AppState>,
    auth: AuthUser,
) -> ApiResult<Json<Value>> {
    require_moderator(&state, auth.id).await?;
    let (count,): (i64,) = sqlx::query_as("SELECT count(*) FROM tickets WHERE resolution IS NULL")
        .fetch_one(&state.pool)
        .await?;
    Ok(Json(json!({ "tickets": count })))
}

async fn severe_open_tickets_count(
    State(state): State<AppState>,
    auth: AuthUser,
) -> ApiResult<Json<Value>> {
    require_moderator(&state, auth.id).await?;
    let (count,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM tickets WHERE severe = true AND resolution IS NULL")
            .fetch_one(&state.pool)
            .await?;
    Ok(Json(json!({ "tickets": count })))
}

async fn assign_new_tickets(
    State(state): State<AppState>,
    auth: AuthUser,
) -> ApiResult<Json<Value>> {
    require_moderator(&state, auth.id).await?;
    Ok(Json(json!({ "newTicketsIds": [] })))
}

/// JS `res.json(0)` — a bare number, not an object.
async fn count_my_tickets(State(state): State<AppState>, auth: AuthUser) -> ApiResult<Json<i32>> {
    require_moderator(&state, auth.id).await?;
    Ok(Json(0))
}

// ---------------------------------------------------------------------------
// POST /moderator/set-ticket-status {ticketId, status, assignedReason?}
// ---------------------------------------------------------------------------

async fn set_ticket_status(
    State(state): State<AppState>,
    auth: AuthUser,
    FlexibleJson(body): FlexibleJson<Value>,
) -> ApiResult<Json<Value>> {
    require_moderator(&state, auth.id).await?;

    let Some(ticket_id) = body.get("ticketId").and_then(Value::as_str) else {
        return Err(ApiError::bad_request("Bad Request"));
    };
    if ticket_id.is_empty() {
        return Err(ApiError::bad_request("Bad Request"));
    }

    let Some(status) = body.get("status").and_then(Value::as_str) else {
        return Err(ApiError::bad_request("Invalid status"));
    };
    let resolution = match status {
        "ignore" => "ignore",
        "timeout" => "timeout",
        "ban" => "ban",
        _ => return Err(ApiError::bad_request("Invalid status")),
    };

    let Ok(uuid) = Uuid::parse_str(ticket_id) else {
        return Err(ApiError::bad_request("Bad Request"));
    };

    let assigned_reason = body
        .get("assignedReason")
        .and_then(Value::as_str)
        .map(String::from);
    ticket::resolve(&state, uuid, auth.id, resolution, assigned_reason).await?;
    Ok(Json(json!({})))
}

// ---------------------------------------------------------------------------
// GET /moderation — moderation.html
// ---------------------------------------------------------------------------

async fn moderation_html(State(state): State<AppState>, auth: AuthUser) -> Response {
    if let Err(err) = require_moderator(&state, auth.id).await {
        return err.into_response();
    }
    match tokio::fs::read_to_string("./frontend/moderation.html").await {
        Ok(html) => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
            html,
        )
            .into_response(),
        Err(_) => (
            StatusCode::NOT_FOUND,
            [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
            "Not found".to_string(),
        )
            .into_response(),
    }
}

// ---------------------------------------------------------------------------
// GET /moderator/{season}/pixel/{tile_x}/{tile_y} — 302 redirect, no auth
// ---------------------------------------------------------------------------

async fn pixel_redirect(
    Path((season, tile_x, tile_y)): Path<(String, String, String)>,
) -> Response {
    let target = format!("/{season}/pixel/{tile_x}/{tile_y}");
    (StatusCode::FOUND, [(header::LOCATION, target)]).into_response()
}

// ---------------------------------------------------------------------------
// GET /moderator/pixel-area/{season}/{tile_x}/{tile_y}?x0&y0&x1&y1
// ---------------------------------------------------------------------------

async fn pixel_area(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((season, tile_x, tile_y)): Path<(String, String, String)>,
    Query(params): Query<HashMap<String, String>>,
) -> ApiResult<Response> {
    require_moderator(&state, auth.id).await?;

    // JS parseInt → NaN for missing/invalid; every check below 400s on NaN.
    let parse = |key: &str| js_parse_int(params.get(key).map(String::as_str).unwrap_or(""));
    let (Some(x0), Some(y0), Some(x1), Some(y1)) =
        (parse("x0"), parse("y0"), parse("x1"), parse("y1"))
    else {
        return Err(ApiError::bad_request("Bad Request"));
    };
    let (x0, y0, x1, y1) = (x0 as i32, y0 as i32, x1 as i32, y1 as i32);

    if y1 < y0 || x1 < x0 {
        return Err(ApiError::bad_request("Bad Request"));
    }

    // validatePixelInfo: season "s0", integer tile coords, pixel corners
    // within 0..TILE_SIZE.
    if season != "s0" {
        return Err(ApiError::bad_request("Bad Request"));
    }
    let (Ok(tile_x), Ok(tile_y)) = (tile_x.parse::<i32>(), tile_y.parse::<i32>()) else {
        return Err(ApiError::bad_request("Bad Request"));
    };
    let valid_pixel = |v: i32| (0..TILE_SIZE).contains(&v);
    if !valid_pixel(x0) || !valid_pixel(y0) || !valid_pixel(x1) || !valid_pixel(y1) {
        return Err(ApiError::bad_request("Bad Request"));
    }

    let rows: Vec<(i16, i16, i32)> = sqlx::query_as(
        "SELECT x, y, painted_by FROM pixels \
         WHERE season = 0 AND tile_x = $1 AND tile_y = $2 \
           AND x BETWEEN $3 AND $4 AND y BETWEEN $5 AND $6",
    )
    .bind(tile_x)
    .bind(tile_y)
    .bind(x0 as i16)
    .bind(x1 as i16)
    .bind(y0 as i16)
    .bind(y1 as i16)
    .fetch_all(&state.pool)
    .await?;
    let mut painted: HashMap<(i16, i16), i32> = HashMap::with_capacity(rows.len());
    for (x, y, id) in rows {
        painted.insert((x, y), id);
    }

    // Row-major u32 LE stream: y outer, x inner (pixelService order).
    let mut buf = Vec::with_capacity(((x1 - x0 + 1) * (y1 - y0 + 1)) as usize * 4);
    for y in y0..=y1 {
        for x in x0..=x1 {
            let id = painted.get(&(x as i16, y as i16)).copied().unwrap_or(0);
            buf.extend_from_slice(&(id as u32).to_le_bytes());
        }
    }

    Ok((
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/octet-stream")],
        buf,
    )
        .into_response())
}
