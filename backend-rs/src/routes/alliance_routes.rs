//! Port of src/routes/alliance.ts + src/services/alliance.ts.
//! Every endpoint requires auth (AuthUser); alliance-admin checks are done
//! manually against users.alliance_role (JS parity, no require_admin).
use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::{json, Map, Value};
use sqlx::FromRow;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::extract::FlexibleJson;
use crate::services::region::pixels_to_lat_lon;
use crate::services::stats::period_start;
use crate::services::user_cache::UserRuntime;
use crate::state::AppState;
use crate::utils::profanity::is_acceptable_username;
use crate::utils::sanitize::escape_html;

const PAGE_SIZE: i64 = 50;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/alliance", get(get_alliance).post(create_alliance))
        .route("/alliance/update-description", post(update_description))
        .route("/alliance/invites", get(get_invites))
        .route("/alliance/join/{invite}", get(join_alliance))
        .route("/alliance/update-headquarters", post(update_headquarters))
        .route("/alliance/members/{page}", get(get_members))
        .route("/alliance/members/banned/{page}", get(get_banned_members))
        .route("/alliance/leave", post(leave_alliance))
        .route("/alliance/give-admin", post(give_admin))
        .route("/alliance/ban", post(ban_user))
        .route("/alliance/unban", post(unban_user))
        .route("/alliance/leaderboard/{mode}", get(alliance_leaderboard))
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

async fn load_user(state: &AppState, user_id: i32) -> ApiResult<Arc<UserRuntime>> {
    state
        .users
        .get_or_load(&state.pool, user_id)
        .await
        .ok_or(ApiError::UserNotFound)
}

/// JS guard `!user || !user.allianceId || user.allianceRole !== "admin"`
/// → Error("Forbidden") → 403 {"error":"Forbidden","status":403}.
async fn require_alliance_admin(
    state: &AppState,
    user_id: i32,
) -> ApiResult<(Arc<UserRuntime>, i32)> {
    let user = load_user(state, user_id).await?;
    let Some(alliance_id) = user.alliance_id else {
        return Err(ApiError::forbidden("Forbidden"));
    };
    if user.alliance_role != "admin" {
        return Err(ApiError::forbidden("Forbidden"));
    }
    Ok((user, alliance_id))
}

fn iso_ms(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn parse_body_id(body: &Value, key: &str) -> ApiResult<i32> {
    body.get(key)
        .and_then(Value::as_i64)
        .and_then(|v| i32::try_from(v).ok())
        .ok_or_else(|| ApiError::bad_request("Bad Request"))
}

// ---------------------------------------------------------------------------
// GET /alliance
// ---------------------------------------------------------------------------

async fn get_alliance(State(state): State<AppState>, auth: AuthUser) -> ApiResult<Json<Value>> {
    let user = load_user(&state, auth.id).await?;
    let Some(alliance_id) = user.alliance_id else {
        return Err(ApiError::NoAlliance);
    };
    let row: Option<(
        i32,
        String,
        Option<String>,
        Option<f64>,
        Option<f64>,
        i32,
        DateTime<Utc>,
        DateTime<Utc>,
    )> = sqlx::query_as(
        "SELECT id, name, description, hq_latitude, hq_longitude, pixels_painted, \
             created_at, updated_at FROM alliances WHERE id = $1",
    )
    .bind(alliance_id)
    .fetch_optional(&state.pool)
    .await?;
    let (_, name, description, hq_lat, hq_lon, pixels, created_at, updated_at) =
        row.ok_or(ApiError::NoAlliance)?;

    let (members,): (i64,) = sqlx::query_as("SELECT count(*) FROM users WHERE alliance_id = $1")
        .bind(alliance_id)
        .fetch_one(&state.pool)
        .await?;

    // JS: hqLatitude && hqLongitude ? {latitude, longitude} : null (0 is falsy).
    let hq = match (hq_lat, hq_lon) {
        (Some(lat), Some(lon)) if lat != 0.0 && lon != 0.0 => {
            json!({ "latitude": lat, "longitude": lon })
        }
        _ => Value::Null,
    };

    Ok(Json(json!({
        "id": alliance_id,
        "name": name,
        "description": description.unwrap_or_default(),
        "hq": hq,
        "members": members,
        "pixelsPainted": pixels,
        "role": user.alliance_role,
        "createdAt": iso_ms(created_at),
        "updatedAt": iso_ms(updated_at),
    })))
}

// ---------------------------------------------------------------------------
// POST /alliance
// ---------------------------------------------------------------------------

async fn create_alliance(
    State(state): State<AppState>,
    auth: AuthUser,
    FlexibleJson(body): FlexibleJson<Value>,
) -> ApiResult<Json<Value>> {
    // Route-level typeof check (JS: "Alliance name is required").
    let Some(name) = body.get("name").and_then(Value::as_str) else {
        return Err(ApiError::bad_request("Alliance name is required"));
    };
    // Service-level empty check (JS ValidationError "empty_name").
    if name.is_empty() {
        return Err(ApiError::bad_request("empty_name"));
    }

    let user = load_user(&state, auth.id).await?;
    if user.alliance_id.is_some() {
        return Err(ApiError::bad_request("Already in alliance"));
    }

    let existing: Option<(i32,)> = sqlx::query_as("SELECT id FROM alliances WHERE name = $1")
        .bind(name)
        .fetch_optional(&state.pool)
        .await?;
    if existing.is_some() {
        return Err(ApiError::bad_request("name_taken"));
    }

    // JS isValidAllianceName: 1..=16 characters.
    let len = name.chars().count();
    if len == 0 || len > 16 {
        return Err(ApiError::bad_request("max_characters"));
    }
    // JS UserService.isAcceptableUsername → same ValidationError message.
    if !is_acceptable_username(name) {
        return Err(ApiError::bad_request("max_characters"));
    }

    let mut tx = state.pool.begin().await?;
    let (alliance_id,): (i32,) = sqlx::query_as(
        "INSERT INTO alliances (name, description, pixels_painted) VALUES ($1, '', 0) RETURNING id",
    )
    .bind(name)
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE users SET alliance_id = $1, alliance_role = 'admin', alliance_joined_at = now() \
         WHERE id = $2",
    )
    .bind(alliance_id)
    .bind(auth.id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    // Keep the runtime user cache authoritative after the write.
    state.users.refresh(&state.pool, auth.id).await;

    Ok(Json(json!({ "id": alliance_id })))
}

// ---------------------------------------------------------------------------
// POST /alliance/update-description
// ---------------------------------------------------------------------------

async fn update_description(
    State(state): State<AppState>,
    auth: AuthUser,
    FlexibleJson(body): FlexibleJson<Value>,
) -> ApiResult<Json<Value>> {
    let (_, alliance_id) = require_alliance_admin(&state, auth.id).await?;

    let description = body
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or("");
    if description.chars().count() > 500 {
        return Err(ApiError::bad_request(
            "Description too long (max 500 characters)",
        ));
    }
    let sanitized = escape_html(description).trim().to_string();

    sqlx::query("UPDATE alliances SET description = $1 WHERE id = $2")
        .bind(sanitized)
        .bind(alliance_id)
        .execute(&state.pool)
        .await?;

    Ok(Json(json!({ "success": true })))
}

// ---------------------------------------------------------------------------
// GET /alliance/invites — returns [inviteId] (a single-element array).
// ---------------------------------------------------------------------------

async fn get_invites(State(state): State<AppState>, auth: AuthUser) -> ApiResult<Json<Value>> {
    let (_, alliance_id) = require_alliance_admin(&state, auth.id).await?;

    let existing: Option<(Uuid,)> = sqlx::query_as(
        "SELECT id FROM alliance_invites WHERE alliance_id = $1 ORDER BY created_at ASC LIMIT 1",
    )
    .bind(alliance_id)
    .fetch_optional(&state.pool)
    .await?;

    let invite_id = match existing {
        Some((id,)) => id,
        None => {
            let id = Uuid::new_v4();
            sqlx::query("INSERT INTO alliance_invites (id, alliance_id) VALUES ($1, $2)")
                .bind(id)
                .bind(alliance_id)
                .execute(&state.pool)
                .await?;
            id
        }
    };

    Ok(Json(json!([invite_id.to_string()])))
}

// ---------------------------------------------------------------------------
// GET /alliance/join/{invite}
// ---------------------------------------------------------------------------

async fn join_alliance(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(invite): Path<String>,
) -> ApiResult<Json<Value>> {
    let user = load_user(&state, auth.id).await?;

    if invite.is_empty() {
        return Err(ApiError::bad_request("Invalid invite"));
    }
    // JS passes the raw string to Prisma and 500s on a malformed uuid; 404 is
    // the sane equivalent here.
    let Ok(invite_id) = Uuid::parse_str(&invite) else {
        return Err(ApiError::NotFound);
    };

    let invite_row: Option<(i32,)> =
        sqlx::query_as("SELECT alliance_id FROM alliance_invites WHERE id = $1")
            .bind(invite_id)
            .fetch_optional(&state.pool)
            .await?;
    let Some((alliance_id,)) = invite_row else {
        return Err(ApiError::NotFound);
    };

    // Already in this alliance → {"success":"true"} (string!).
    if user.alliance_id == Some(alliance_id) {
        return Ok(Json(json!({ "success": "true" })));
    }
    // In a different alliance → 208.
    if user.alliance_id.is_some() {
        return Err(ApiError::AlreadyReported);
    }

    let banned: Option<(i32,)> = sqlx::query_as(
        "SELECT id FROM alliance_banned_users WHERE user_id = $1 AND alliance_id = $2",
    )
    .bind(auth.id)
    .bind(alliance_id)
    .fetch_optional(&state.pool)
    .await?;
    if banned.is_some() {
        return Err(ApiError::forbidden("Forbidden"));
    }

    sqlx::query(
        "UPDATE users SET alliance_id = $1, alliance_role = 'member', alliance_joined_at = now() \
         WHERE id = $2",
    )
    .bind(alliance_id)
    .bind(auth.id)
    .execute(&state.pool)
    .await?;
    state.users.refresh(&state.pool, auth.id).await;

    Ok(Json(json!({ "success": "true" })))
}

// ---------------------------------------------------------------------------
// POST /alliance/update-headquarters
// ---------------------------------------------------------------------------

async fn update_headquarters(
    State(state): State<AppState>,
    auth: AuthUser,
    FlexibleJson(body): FlexibleJson<Value>,
) -> ApiResult<Json<Value>> {
    let Some(latitude) = body.get("latitude").and_then(Value::as_f64) else {
        return Err(ApiError::bad_request("Bad Request"));
    };
    let Some(longitude) = body.get("longitude").and_then(Value::as_f64) else {
        return Err(ApiError::bad_request("Bad Request"));
    };

    let (_, alliance_id) = require_alliance_admin(&state, auth.id).await?;

    sqlx::query("UPDATE alliances SET hq_latitude = $1, hq_longitude = $2 WHERE id = $3")
        .bind(latitude)
        .bind(longitude)
        .bind(alliance_id)
        .execute(&state.pool)
        .await?;

    Ok(Json(json!({ "success": true })))
}

// ---------------------------------------------------------------------------
// GET /alliance/members/{page} and /alliance/members/banned/{page}
// ---------------------------------------------------------------------------

async fn get_members(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(page): Path<String>,
) -> ApiResult<Json<Value>> {
    // JS: Number.parseInt(x) || 0; negative pages are rejected.
    let page: i64 = page.parse::<i32>().map(i64::from).unwrap_or(0);
    if page < 0 {
        return Err(ApiError::bad_request("Bad Request"));
    }
    let (_, alliance_id) = require_alliance_admin(&state, auth.id).await?;

    let rows: Vec<(i32, String, Option<String>, Option<String>, String)> = sqlx::query_as(
        "SELECT id, name, nickname, picture, alliance_role FROM users \
         WHERE alliance_id = $1 ORDER BY id LIMIT 51 OFFSET $2",
    )
    .bind(alliance_id)
    .bind(page * PAGE_SIZE)
    .fetch_all(&state.pool)
    .await?;

    let has_next = rows.len() as i64 > PAGE_SIZE;
    let data: Vec<Value> = rows
        .into_iter()
        .take(PAGE_SIZE as usize)
        .map(|(id, name, nickname, picture, role)| {
            let mut o = Map::new();
            o.insert("id".into(), json!(id));
            o.insert(
                "name".into(),
                json!(nickname.filter(|n| !n.is_empty()).unwrap_or(name)),
            );
            if let Some(p) = picture.filter(|p| !p.is_empty()) {
                o.insert("picture".into(), json!(p));
            }
            o.insert("role".into(), json!(role));
            Value::Object(o)
        })
        .collect();

    Ok(Json(json!({ "data": data, "hasNext": has_next })))
}

#[derive(FromRow)]
struct BannedUserRow {
    id: i32,
    nickname: Option<String>,
    name: String,
    picture: Option<String>,
}

async fn get_banned_members(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(page): Path<String>,
) -> ApiResult<Json<Value>> {
    let page: i64 = page.parse::<i32>().map(i64::from).unwrap_or(0);
    if page < 0 {
        return Err(ApiError::bad_request("Bad Request"));
    }
    let (_, alliance_id) = require_alliance_admin(&state, auth.id).await?;

    let rows: Vec<(i32,)> = sqlx::query_as(
        "SELECT user_id FROM alliance_banned_users \
         WHERE alliance_id = $1 ORDER BY id LIMIT 51 OFFSET $2",
    )
    .bind(alliance_id)
    .bind(page * PAGE_SIZE)
    .fetch_all(&state.pool)
    .await?;

    let has_next = rows.len() as i64 > PAGE_SIZE;
    let ids: Vec<i32> = rows
        .into_iter()
        .take(PAGE_SIZE as usize)
        .map(|(id,)| id)
        .collect();

    let mut users: HashMap<i32, BannedUserRow> = HashMap::new();
    if !ids.is_empty() {
        let fetched: Vec<BannedUserRow> =
            sqlx::query_as("SELECT id, nickname, name, picture FROM users WHERE id = ANY($1)")
                .bind(&ids)
                .fetch_all(&state.pool)
                .await?;
        for u in fetched {
            users.insert(u.id, u);
        }
    }

    // JS: picture is always present here (null when missing).
    let data: Vec<Value> = ids
        .iter()
        .map(|id| {
            let u = users.get(id);
            let name = match u {
                Some(u) => u
                    .nickname
                    .as_deref()
                    .filter(|n| !n.is_empty())
                    .unwrap_or(&u.name)
                    .to_string(),
                None => "Unknown".to_string(),
            };
            json!({
                "id": id,
                "name": name,
                "picture": u.and_then(|u| u.picture.clone()),
            })
        })
        .collect();

    Ok(Json(json!({ "data": data, "hasNext": has_next })))
}

// ---------------------------------------------------------------------------
// POST /alliance/leave
// ---------------------------------------------------------------------------

async fn leave_alliance(State(state): State<AppState>, auth: AuthUser) -> ApiResult<Json<Value>> {
    let user = load_user(&state, auth.id).await?;
    let Some(alliance_id) = user.alliance_id else {
        return Err(ApiError::forbidden("Forbidden"));
    };

    if user.alliance_role == "admin" {
        let (admin_count,): (i64,) = sqlx::query_as(
            "SELECT count(*) FROM users WHERE alliance_id = $1 AND alliance_role = 'admin'",
        )
        .bind(alliance_id)
        .fetch_one(&state.pool)
        .await?;

        // Last admin hands the role to the first member (JS findFirst).
        if admin_count <= 1 {
            let new_admin: Option<(i32,)> = sqlx::query_as(
                "SELECT id FROM users WHERE alliance_id = $1 AND alliance_role = 'member' \
                 ORDER BY id LIMIT 1",
            )
            .bind(alliance_id)
            .fetch_optional(&state.pool)
            .await?;
            if let Some((new_admin_id,)) = new_admin {
                sqlx::query("UPDATE users SET alliance_role = 'admin' WHERE id = $1")
                    .bind(new_admin_id)
                    .execute(&state.pool)
                    .await?;
                state.users.refresh(&state.pool, new_admin_id).await;
            }
        }
    }

    sqlx::query(
        "UPDATE users SET alliance_id = NULL, alliance_role = 'member', alliance_joined_at = NULL \
         WHERE id = $1",
    )
    .bind(auth.id)
    .execute(&state.pool)
    .await?;
    state.users.refresh(&state.pool, auth.id).await;

    Ok(Json(json!({ "success": true })))
}

// ---------------------------------------------------------------------------
// POST /alliance/give-admin — 200 with an empty object.
// ---------------------------------------------------------------------------

async fn give_admin(
    State(state): State<AppState>,
    auth: AuthUser,
    FlexibleJson(body): FlexibleJson<Value>,
) -> ApiResult<Json<Value>> {
    let (_, alliance_id) = require_alliance_admin(&state, auth.id).await?;
    let promoted_user_id = parse_body_id(&body, "promotedUserId")?;

    // JS updates where {id, allianceId} — no-op (empty response) otherwise.
    sqlx::query("UPDATE users SET alliance_role = 'admin' WHERE id = $1 AND alliance_id = $2")
        .bind(promoted_user_id)
        .bind(alliance_id)
        .execute(&state.pool)
        .await?;
    state.users.refresh(&state.pool, promoted_user_id).await;

    Ok(Json(json!({})))
}

// ---------------------------------------------------------------------------
// POST /alliance/ban — kick + ban; empty alliance is deleted.
// ---------------------------------------------------------------------------

async fn ban_user(
    State(state): State<AppState>,
    auth: AuthUser,
    FlexibleJson(body): FlexibleJson<Value>,
) -> ApiResult<Json<Value>> {
    let (_, alliance_id) = require_alliance_admin(&state, auth.id).await?;
    let banned_user_id = parse_body_id(&body, "bannedUserId")?;

    let mut tx = state.pool.begin().await?;

    sqlx::query("UPDATE users SET alliance_id = NULL, alliance_role = 'member' WHERE id = $1")
        .bind(banned_user_id)
        .execute(&mut *tx)
        .await?;

    sqlx::query(
        "INSERT INTO alliance_banned_users (user_id, alliance_id) VALUES ($1, $2) \
         ON CONFLICT DO NOTHING",
    )
    .bind(banned_user_id)
    .bind(alliance_id)
    .execute(&mut *tx)
    .await?;

    let (remaining,): (i64,) = sqlx::query_as("SELECT count(*) FROM users WHERE alliance_id = $1")
        .bind(alliance_id)
        .fetch_one(&mut *tx)
        .await?;

    // No members left → tear the alliance down (invites, bans, then the row).
    if remaining == 0 {
        sqlx::query("DELETE FROM alliance_invites WHERE alliance_id = $1")
            .bind(alliance_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM alliance_banned_users WHERE alliance_id = $1")
            .bind(alliance_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM alliances WHERE id = $1")
            .bind(alliance_id)
            .execute(&mut *tx)
            .await?;
    }

    tx.commit().await?;

    // FK ON DELETE SET NULL clears the admin's alliance_id on teardown.
    state.users.refresh(&state.pool, banned_user_id).await;
    state.users.refresh(&state.pool, auth.id).await;

    Ok(Json(json!({ "success": true })))
}

// ---------------------------------------------------------------------------
// POST /alliance/unban
// ---------------------------------------------------------------------------

async fn unban_user(
    State(state): State<AppState>,
    auth: AuthUser,
    FlexibleJson(body): FlexibleJson<Value>,
) -> ApiResult<Json<Value>> {
    let (_, alliance_id) = require_alliance_admin(&state, auth.id).await?;
    let unbanned_user_id = parse_body_id(&body, "unbannedUserId")?;

    sqlx::query("DELETE FROM alliance_banned_users WHERE user_id = $1 AND alliance_id = $2")
        .bind(unbanned_user_id)
        .bind(alliance_id)
        .execute(&state.pool)
        .await?;

    Ok(Json(json!({ "success": true })))
}

// ---------------------------------------------------------------------------
// GET /alliance/leaderboard/{mode}
// ---------------------------------------------------------------------------

#[derive(FromRow)]
struct MemberPixelsRow {
    id: i32,
    name: String,
    picture: Option<String>,
    equipped_flag: i32,
    show_last_pixel: bool,
    painted: i64,
}

#[derive(FromRow)]
struct LastPixelRow {
    painted_by: i32,
    tile_x: i32,
    tile_y: i32,
    x: i16,
    y: i16,
}

async fn alliance_leaderboard(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(mode): Path<String>,
) -> ApiResult<Json<Value>> {
    let user = load_user(&state, auth.id).await?;
    let Some(alliance_id) = user.alliance_id else {
        return Err(ApiError::forbidden("Forbidden"));
    };

    // One aggregate over all members instead of the JS N+1 per-member counts:
    // painted_at >= GREATEST(alliance_joined_at, period start); for all-time
    // (or an unknown mode) $2 is NULL and GREATEST ignores NULLs, leaving the
    // join-date floor only.
    let start = period_start(&mode);
    let rows: Vec<MemberPixelsRow> = sqlx::query_as(
        r#"
        SELECT u.id,
               COALESCE(u.nickname, u.name) AS name,
               u.picture, u.equipped_flag, u.show_last_pixel,
               COUNT(p.id) AS painted
        FROM users u
        LEFT JOIN pixels p ON p.painted_by = u.id
            AND p.painted_at >= GREATEST(u.alliance_joined_at, $2::timestamptz)
        WHERE u.alliance_id = $1
          AND u.alliance_joined_at IS NOT NULL
        GROUP BY u.id, u.nickname, u.name, u.picture,
                 u.equipped_flag, u.show_last_pixel
        ORDER BY painted DESC, u.id ASC
        "#,
    )
    .bind(alliance_id)
    .bind(start)
    .fetch_all(&state.pool)
    .await?;

    let valid: Vec<&MemberPixelsRow> = rows.iter().filter(|m| m.painted > 0).take(50).collect();

    // Last pixel per member (JS findFirst paintedAt desc, id desc) in one
    // DISTINCT ON query.
    let member_ids: Vec<i32> = valid.iter().map(|m| m.id).collect();
    let mut last_pixels: HashMap<i32, LastPixelRow> = HashMap::new();
    if !member_ids.is_empty() {
        let lasts: Vec<LastPixelRow> = sqlx::query_as(
            "SELECT DISTINCT ON (painted_by) painted_by, tile_x, tile_y, x, y \
             FROM pixels WHERE painted_by = ANY($1) \
             ORDER BY painted_by, painted_at DESC, id DESC",
        )
        .bind(&member_ids)
        .fetch_all(&state.pool)
        .await?;
        for l in lasts {
            last_pixels.insert(l.painted_by, l);
        }
    }

    let entries: Vec<Value> = valid
        .iter()
        .map(|m| {
            let mut o = Map::new();
            o.insert("userId".into(), json!(m.id));
            o.insert("name".into(), json!(m.name));
            if let Some(p) = m.picture.as_deref().filter(|p| !p.is_empty()) {
                o.insert("picture".into(), json!(p));
            }
            o.insert("equippedFlag".into(), json!(m.equipped_flag));
            o.insert("pixelsPainted".into(), json!(m.painted));
            if m.show_last_pixel {
                if let Some(lp) = last_pixels.get(&m.id) {
                    let (lat, lon) =
                        pixels_to_lat_lon(lp.tile_x, lp.tile_y, lp.x as i32, lp.y as i32);
                    o.insert("lastLatitude".into(), json!(lat));
                    o.insert("lastLongitude".into(), json!(lon));
                }
            }
            Value::Object(o)
        })
        .collect();

    Ok(Json(Value::Array(entries)))
}
