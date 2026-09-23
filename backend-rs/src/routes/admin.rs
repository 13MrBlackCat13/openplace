// Admin endpoints — port of src/routes/admin.ts. Every route requires auth +
// require_admin (adminMiddleware).
use std::collections::HashMap;

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

use crate::auth::{require_admin, require_moderator, AuthUser};
use crate::error::{ApiError, ApiResult};
use crate::extract::FlexibleJson;
use crate::routes::report::js_parse_int;
use crate::services::settings::{self, RuntimeSettings};
use crate::services::user_ops;
use crate::state::AppState;

/// JS Number.MAX_SAFE_INTEGER.
const MAX_SAFE_INTEGER: f64 = 9_007_199_254_740_991.0;
/// REPORT_REASONS keys from admin.ts (fixed response keys; unknown reasons are
/// added on top, matching the JS Map behaviour).
const REPORT_REASONS: [&str; 6] = [
    "doxxing",
    "inappropriate_content",
    "hate_speech",
    "bot",
    "other",
    "griefing",
];

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/admin", get(admin_html))
        .route("/admin/customize", get(customize_html))
        .route("/admin/settings", get(get_settings).post(update_settings))
        .route("/admin/users", get(admin_user))
        .route("/admin/users/notes", get(get_notes).post(create_note))
        // The newer admin UI calls the moderator-prefixed variants.
        .route(
            "/moderator/users/notes",
            get(get_notes_moderator).post(create_note_moderator),
        )
        .route("/admin/event/status", get(event_status))
        .route("/admin/users/tickets", get(user_ticket_counts))
        .route("/admin/users/purchases", get(user_purchases))
        .route("/admin/users/set-user-droplets", post(set_user_droplets))
        .route("/admin/tickets", get(list_tickets_open))
        .route("/admin/closed-tickets", get(list_tickets_closed))
        .route("/admin/closed-reports", get(list_tickets_closed))
        .route("/admin/open-tickets-count", get(open_tickets_count))
        .route(
            "/admin/severe-open-tickets-count",
            post(severe_open_tickets_count),
        )
        .route("/admin/assign-new-tickets", post(assign_new_tickets))
        .route("/admin/count-all-tickets", get(count_all_tickets))
        .route("/admin/count-all-reports", get(count_all_reports))
        .route("/admin/alliances/search", get(search_alliances))
        .route("/admin/alliances/{id}", get(get_alliance))
        .route("/admin/alliances/{id}/full", get(get_alliance_full))
        .route("/admin/remove-ban", post(remove_ban))
        .route("/admin/remove-timeout", post(remove_timeout))
        // JS quirk: this route lives in admin.ts but under the /moderator path,
        // and is guarded by adminMiddleware (not moderatorMiddleware).
        .route("/moderator/remove-timeout", post(remove_timeout))
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

/// JS `Number.parseInt(query) || 0` + `id <= 0 → 400`.
fn parse_positive_query(params: &HashMap<String, String>, key: &str) -> ApiResult<i64> {
    let raw = params.get(key).map(String::as_str).unwrap_or("");
    match js_parse_int(raw) {
        Some(v) if v > 0.0 => Ok(v as i64),
        _ => Err(ApiError::bad_request("Bad Request")),
    }
}

/// Narrow a validated query id to users.id (int4); beyond range → not found.
fn as_user_id(v: i64) -> ApiResult<i32> {
    i32::try_from(v).map_err(|_| ApiError::UserNotFound)
}

/// JS `Number.parseInt(req.body[k] ?? "") || 0` — accepts JSON numbers too.
fn json_parse_int(v: Option<&Value>) -> Option<i64> {
    match v {
        Some(Value::Number(n)) => match n.as_i64() {
            Some(i) => Some(i),
            None => n.as_f64().map(|f| f.trunc() as i64),
        },
        Some(Value::String(s)) => js_parse_int(s).map(|f| f as i64),
        _ => None,
    }
}

async fn user_exists(state: &AppState, user_id: i32) -> ApiResult<bool> {
    let found: Option<(i32,)> = sqlx::query_as("SELECT id FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_optional(&state.pool)
        .await?;
    Ok(found.is_some())
}

// ---------------------------------------------------------------------------
// GET /admin/users?id=
// ---------------------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct AdminUserRow {
    id: i32,
    name: String,
    nickname: Option<String>,
    droplets: i32,
    picture: Option<String>,
    role: String,
    timeout_until: DateTime<Utc>,
    suspension_reason: Option<String>,
    last_ip: Option<String>,
    registration_ip: Option<String>,
    alliance_id: Option<i32>,
    pixels_painted: i32,
    discord: Option<String>,
}

async fn admin_user(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(params): Query<HashMap<String, String>>,
) -> ApiResult<Json<Value>> {
    require_admin(&state, auth.id).await?;

    let Some(id) = js_parse_int(params.get("id").map(String::as_str).unwrap_or("")) else {
        return Err(ApiError::bad_request("Bad Request"));
    };
    if id <= 0.0 || id > MAX_SAFE_INTEGER {
        return Err(ApiError::bad_request("Bad Request"));
    }
    let id = as_user_id(id as i64)?;

    let Some(user) = sqlx::query_as::<_, AdminUserRow>(
        "SELECT id, name, nickname, droplets, picture, role, timeout_until, suspension_reason, \
                last_ip, registration_ip, alliance_id, pixels_painted, discord \
         FROM users WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await?
    else {
        return Err(ApiError::UserNotFound);
    };

    let alliance_name: Option<String> = match user.alliance_id {
        Some(aid) => sqlx::query_as("SELECT name FROM alliances WHERE id = $1")
            .bind(aid)
            .fetch_optional(&state.pool)
            .await?
            .map(|(name,)| name),
        None => None,
    };

    let (reported_times, timeouts_count): (i64, i64) = sqlx::query_as(
        "SELECT count(*), count(*) FILTER (WHERE resolution = 'timeout') \
         FROM tickets WHERE reported_user_id = $1",
    )
    .bind(id)
    .fetch_one(&state.pool)
    .await?;

    let (same_ip_accounts,): (i64,) = sqlx::query_as(
        "SELECT count(*) FROM users \
         WHERE id <> $1 AND (($2 IS NOT NULL AND last_ip = $2) OR ($3 IS NOT NULL AND registration_ip = $3))",
    )
    .bind(id)
    .bind(&user.last_ip)
    .bind(&user.registration_ip)
    .fetch_one(&state.pool)
    .await?;

    // Anti-automation view: score + reasons, PoW trust deadline, and the
    // fingerprint linkage (distinct visitor count + other accounts sharing
    // those visitors). Static shape when the system is off (runtime mode).
    let runtime_settings = state.settings_snapshot();
    let (bot_score, bot_reasons, trusted_until, fp_accounts, fp_linked_users) =
        if runtime_settings.anti_bot_mode == "off" {
            (0, Vec::new(), None, 0i64, Vec::new())
        } else {
            let flag: Option<(i32, Vec<String>)> =
                sqlx::query_as("SELECT score, reasons FROM bot_flags WHERE user_id = $1")
                    .bind(id)
                    .fetch_optional(&state.pool)
                    .await?;
            let (bot_score, bot_reasons) = flag.unwrap_or((0, Vec::new()));
            let trusted_until = state.antibot.trusted_remaining(id).map(|remaining| {
                iso_ms(Utc::now() + chrono::Duration::milliseconds(remaining.as_millis() as i64))
            });
            let (fp_accounts,): (i64,) = sqlx::query_as(
                "SELECT count(DISTINCT visitor_id) FROM visitor_accounts WHERE user_id = $1",
            )
            .bind(id)
            .fetch_one(&state.pool)
            .await?;
            let linked: Vec<(i32,)> = sqlx::query_as(
                "SELECT DISTINCT va2.user_id FROM visitor_accounts va1 \
                 JOIN visitor_accounts va2 \
                   ON va2.visitor_id = va1.visitor_id AND va2.user_id <> va1.user_id \
                 WHERE va1.user_id = $1 LIMIT 10",
            )
            .bind(id)
            .fetch_all(&state.pool)
            .await?;
            (
                bot_score,
                bot_reasons,
                trusted_until,
                fp_accounts,
                linked.into_iter().map(|(u,)| u).collect(),
            )
        };

    Ok(Json(json!({
        "userId": user.id,
        "id": user.id,
        "name": display_name(&user.nickname, &user.name),
        "droplets": user.droplets,
        "picture": user.picture.unwrap_or_default(),
        "role": user.role,
        "timeout_until": iso_ms(user.timeout_until),
        "ban_reason": user.suspension_reason,
        "reported_times": reported_times,
        "timeouts_count": timeouts_count,
        "same_ip_accounts": same_ip_accounts,
        "alliance_id": user.alliance_id.unwrap_or(0),
        "alliance_name": alliance_name.unwrap_or_default(),
        "pixels_painted": user.pixels_painted,
        "phone_validated": false,
        "discord": user.discord.unwrap_or_default(),
        "bot": {
            "score": bot_score,
            "reasons": bot_reasons,
            "trusted_until": trusted_until,
        },
        "fp_accounts": fp_accounts,
        "fp_linked_users": fp_linked_users,
    })))
}

// ---------------------------------------------------------------------------
// GET/POST /admin/users/notes + /moderator/users/notes
// (the newer admin UI calls the /moderator variant)
// ---------------------------------------------------------------------------

async fn get_notes(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(params): Query<HashMap<String, String>>,
) -> ApiResult<Json<Value>> {
    require_admin(&state, auth.id).await?;
    get_notes_inner(&state, &params).await
}

async fn get_notes_moderator(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(params): Query<HashMap<String, String>>,
) -> ApiResult<Json<Value>> {
    require_moderator(&state, auth.id).await?;
    get_notes_inner(&state, &params).await
}

async fn get_notes_inner(
    state: &AppState,
    params: &HashMap<String, String>,
) -> ApiResult<Json<Value>> {
    let user_id = as_user_id(parse_positive_query(params, "userId")?)?;
    if !user_exists(state, user_id).await? {
        return Err(ApiError::UserNotFound);
    }

    let rows: Vec<(
        i32,
        String,
        String,
        Option<String>,
        DateTime<Utc>,
        i32,
        String,
    )> = sqlx::query_as(
        "SELECT un.id, un.content, u.name, u.nickname, un.created_at, u.id, u.role \
             FROM user_notes un JOIN users u ON u.id = un.user_id \
             WHERE un.reported_user_id = $1 ORDER BY un.created_at DESC",
    )
    .bind(user_id)
    .fetch_all(&state.pool)
    .await?;

    let notes: Vec<Value> = rows
        .into_iter()
        .map(
            |(id, content, author_name, author_nickname, created_at, author_id, role)| {
                json!({
                    "id": id,
                    "author": {
                        "role": role,
                        "id": author_id,
                        "name": display_name(&author_nickname, &author_name),
                    },
                    "note": content,
                    "createdAt": iso_ms(created_at),
                })
            },
        )
        .collect();
    Ok(Json(json!({ "notes": notes })))
}

async fn create_note(
    State(state): State<AppState>,
    auth: AuthUser,
    FlexibleJson(body): FlexibleJson<Value>,
) -> ApiResult<Json<Value>> {
    require_admin(&state, auth.id).await?;
    create_note_inner(&state, auth.id, body).await
}

async fn create_note_moderator(
    State(state): State<AppState>,
    auth: AuthUser,
    FlexibleJson(body): FlexibleJson<Value>,
) -> ApiResult<Json<Value>> {
    require_moderator(&state, auth.id).await?;
    create_note_inner(&state, auth.id, body).await
}

async fn create_note_inner(
    state: &AppState,
    author_id: i32,
    body: Value,
) -> ApiResult<Json<Value>> {
    let user_id = json_parse_int(body.get("userId")).unwrap_or(0);
    let note = body.get("note").and_then(Value::as_str);
    if user_id <= 0 || note.is_none() {
        return Err(ApiError::bad_request("Bad Request"));
    }
    let Ok(user_id) = i32::try_from(user_id) else {
        return Err(ApiError::UserNotFound);
    };
    if !user_exists(state, user_id).await? {
        return Err(ApiError::UserNotFound);
    }

    sqlx::query("INSERT INTO user_notes (user_id, reported_user_id, content) VALUES ($1, $2, $3)")
        .bind(author_id)
        .bind(user_id)
        .bind(note)
        .execute(&state.pool)
        .await?;
    Ok(Json(json!({})))
}

// ---------------------------------------------------------------------------
// GET /admin/users/tickets?id=  — global + per-reported-user counters
// ---------------------------------------------------------------------------

async fn user_ticket_counts(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(params): Query<HashMap<String, String>>,
) -> ApiResult<Json<Value>> {
    require_admin(&state, auth.id).await?;
    let user_id = as_user_id(parse_positive_query(&params, "id")?)?;
    if !user_exists(&state, user_id).await? {
        return Err(ApiError::UserNotFound);
    }

    let (g_closed, g_ignored, g_timeouts, g_bans): (i64, i64, i64, i64) = sqlx::query_as(
        "SELECT count(*) FILTER (WHERE resolution IS NOT NULL), \
                count(*) FILTER (WHERE resolution = 'ignore'), \
                count(*) FILTER (WHERE resolution = 'timeout'), \
                count(*) FILTER (WHERE resolution = 'ban') \
         FROM tickets",
    )
    .fetch_one(&state.pool)
    .await?;

    let (r_closed, r_ignored, r_timeouts, r_bans): (i64, i64, i64, i64) = sqlx::query_as(
        "SELECT count(*) FILTER (WHERE resolution IS NOT NULL), \
                count(*) FILTER (WHERE resolution = 'ignore'), \
                count(*) FILTER (WHERE resolution = 'timeout'), \
                count(*) FILTER (WHERE resolution = 'ban') \
         FROM tickets WHERE reported_user_id = $1",
    )
    .bind(user_id)
    .fetch_one(&state.pool)
    .await?;

    Ok(Json(json!({
        "closedTotal": g_closed,
        "ignored": g_ignored,
        "timeouts": g_timeouts,
        "bans": g_bans,
        "rclosedTotal": r_closed,
        "rignored": r_ignored,
        "rtimeouts": r_timeouts,
        "rbans": r_bans,
    })))
}

// ---------------------------------------------------------------------------
// GET /admin/users/purchases?userId= — stub, checks only
// ---------------------------------------------------------------------------

async fn user_purchases(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(params): Query<HashMap<String, String>>,
) -> ApiResult<Json<Value>> {
    require_admin(&state, auth.id).await?;
    let user_id = as_user_id(parse_positive_query(&params, "userId")?)?;
    if !user_exists(&state, user_id).await? {
        return Err(ApiError::UserNotFound);
    }
    Ok(Json(json!({})))
}

// ---------------------------------------------------------------------------
// POST /admin/users/set-user-droplets {userId, droplets}
// ---------------------------------------------------------------------------

async fn set_user_droplets(
    State(state): State<AppState>,
    auth: AuthUser,
    FlexibleJson(body): FlexibleJson<Value>,
) -> ApiResult<Json<Value>> {
    require_admin(&state, auth.id).await?;

    // JS: both parseInt || 0; only userId has an upper-bound check. droplets
    // may be negative; unparseable → 0 (the NaN check is dead in JS).
    let user_id = json_parse_int(body.get("userId")).unwrap_or(0);
    let droplets = json_parse_int(body.get("droplets")).unwrap_or(0);
    if user_id <= 0 {
        return Err(ApiError::bad_request("Bad Request"));
    }
    let Ok(user_id) = i32::try_from(user_id) else {
        return Err(ApiError::UserNotFound);
    };
    if !user_exists(&state, user_id).await? {
        return Err(ApiError::UserNotFound);
    }

    let droplets = i32::try_from(droplets).map_err(|_| ApiError::Internal)?;
    sqlx::query("UPDATE users SET droplets = droplets + $2, updated_at = now() WHERE id = $1")
        .bind(user_id)
        .bind(droplets)
        .execute(&state.pool)
        .await?;
    state.users.invalidate(user_id);
    Ok(Json(json!({ "success": true })))
}

// ---------------------------------------------------------------------------
// GET /admin/tickets | /admin/closed-tickets | /admin/closed-reports
// ---------------------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct AdminTicketRow {
    id: Uuid,
    reported_user_id: Option<i32>,
    latitude: f64,
    longitude: f64,
    zoom: f64,
    reason: String,
    notes: String,
    image: Option<Vec<u8>>,
    created_at: DateTime<Utc>,
}

async fn list_tickets_open(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(params): Query<HashMap<String, String>>,
) -> ApiResult<Json<Value>> {
    list_tickets(state, auth, params, false).await
}

async fn list_tickets_closed(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(params): Query<HashMap<String, String>>,
) -> ApiResult<Json<Value>> {
    list_tickets(state, auth, params, true).await
}

async fn list_tickets(
    state: AppState,
    auth: AuthUser,
    params: HashMap<String, String>,
    resolved: bool,
) -> ApiResult<Json<Value>> {
    require_admin(&state, auth.id).await?;

    // closed-reports + both dates → per-moderator statistics.
    if resolved {
        if let (Some(start), Some(end)) = (params.get("start"), params.get("end")) {
            // JS `new Date(garbage)` made prisma throw → 500. Accepts full
            // RFC3339 and date-only (UTC midnight) like new Date("YYYY-MM-DD").
            let parse = |raw: &str| -> ApiResult<DateTime<Utc>> {
                if let Ok(d) = DateTime::parse_from_rfc3339(raw) {
                    return Ok(d.with_timezone(&Utc));
                }
                chrono::NaiveDate::parse_from_str(raw, "%Y-%m-%d")
                    .map(|d| d.and_hms_opt(0, 0, 0).expect("midnight is valid").and_utc())
                    .map_err(|_| ApiError::Internal)
            };
            let start = parse(start)?;
            let end = parse(end)?;

            let rows: Vec<(i32, String, Option<String>, String, i64, i64, i64, i64)> =
                sqlx::query_as(
                    r#"
                    SELECT u.id, u.name, u.nickname, u.role,
                           count(t.id) AS total,
                           count(t.id) FILTER (WHERE t.resolution = 'ban'),
                           count(t.id) FILTER (WHERE t.resolution = 'ignore'),
                           count(t.id) FILTER (WHERE t.resolution = 'timeout')
                    FROM users u
                    LEFT JOIN tickets t
                           ON t.moderator_user_id = u.id
                          AND t.created_at >= $1 AND t.created_at <= $2
                    WHERE u.role = 'moderator'
                    GROUP BY u.id, u.name, u.nickname, u.role
                    ORDER BY u.id
                    "#,
                )
                .bind(start)
                .bind(end)
                .fetch_all(&state.pool)
                .await?;

            let items: Vec<Value> = rows
                .into_iter()
                .filter(|(_, _, _, _, total, _, _, _)| *total > 0)
                .map(|(id, name, nickname, role, total, ban, ignored, timeout)| {
                    json!({
                        "user": {
                            "id": id,
                            "name": display_name(&nickname, &name),
                            "role": role,
                        },
                        "total": total,
                        "ban": ban,
                        "ignored": ignored,
                        "timeout": timeout,
                        "suspensionRate": (ban + timeout) as f64 / total as f64,
                    })
                })
                .collect();
            return Ok(Json(json!({ "items": items })));
        }
    }

    let rows: Vec<AdminTicketRow> = sqlx::query_as(&format!(
        "SELECT id, reported_user_id, latitude, longitude, zoom, reason, notes, image, created_at \
         FROM tickets WHERE resolution {} ORDER BY created_at DESC LIMIT 100",
        if resolved { "IS NOT NULL" } else { "IS NULL" },
    ))
    .fetch_all(&state.pool)
    .await?;

    // Group by reported user, preserving the created_at DESC order of first
    // appearance (JS Map semantics). A deleted reported user groups under
    // null (FK ON DELETE SET NULL).
    let mut order: Vec<Option<i32>> = Vec::new();
    let mut groups: HashMap<Option<i32>, Vec<usize>> = HashMap::new();
    for (i, t) in rows.iter().enumerate() {
        if !groups.contains_key(&t.reported_user_id) {
            order.push(t.reported_user_id);
        }
        groups.entry(t.reported_user_id).or_default().push(i);
    }

    let reported_ids: Vec<i32> = order.iter().flatten().copied().collect();
    let mut user_map: HashMap<i32, (i32, String, Option<String>, Option<String>, String, bool)> =
        HashMap::new();
    if !reported_ids.is_empty() {
        let found: Vec<(i32, String, Option<String>, Option<String>, String, bool)> =
            sqlx::query_as(
                "SELECT id, name, nickname, discord, country, banned FROM users WHERE id = ANY($1)",
            )
            .bind(&reported_ids)
            .fetch_all(&state.pool)
            .await?;
        for row in found {
            user_map.insert(row.0, row);
        }
    }

    let tickets: Vec<Value> = order
        .iter()
        .map(|rid| {
            let idxs = groups.get(rid).expect("group exists");
            let first = &rows[idxs[0]];
            let reported_json = rid.as_ref().and_then(|id| user_map.get(id)).map(
                |(id, name, nickname, discord, country, banned): &(
                    i32,
                    String,
                    Option<String>,
                    Option<String>,
                    String,
                    bool,
                )| {
                    json!({
                        "id": id,
                        "name": display_name(nickname, name),
                        "discord": discord.clone().unwrap_or_default(),
                        "country": country,
                        "banned": banned,
                    })
                },
            );
            let reports: Vec<Value> = idxs
                .iter()
                .map(|&i| {
                    let t = &rows[i];
                    json!({
                        "id": t.id.to_string(),
                        "latitude": t.latitude,
                        "longitude": t.longitude,
                        "zoom": t.zoom,
                        "reason": t.reason,
                        "notes": t.notes,
                        "image": t.image.as_ref().map(|b| BASE64.encode(b)).unwrap_or_default(),
                        "createdAt": iso_ms(t.created_at),
                    })
                })
                .collect();
            json!({
                "id": rid,
                "reportedUser": reported_json,
                "createdAt": iso_ms(first.created_at),
                "reports": reports,
            })
        })
        .collect();

    Ok(Json(json!({ "tickets": tickets, "status": 200 })))
}

// ---------------------------------------------------------------------------
// Ticket counters / stubs
// ---------------------------------------------------------------------------

async fn open_tickets_count(
    State(state): State<AppState>,
    auth: AuthUser,
) -> ApiResult<Json<Value>> {
    require_admin(&state, auth.id).await?;
    let (count,): (i64,) = sqlx::query_as("SELECT count(*) FROM tickets WHERE resolution IS NULL")
        .fetch_one(&state.pool)
        .await?;
    Ok(Json(json!({ "tickets": count })))
}

async fn severe_open_tickets_count(
    State(state): State<AppState>,
    auth: AuthUser,
) -> ApiResult<Json<Value>> {
    require_admin(&state, auth.id).await?;
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
    require_admin(&state, auth.id).await?;
    Ok(Json(json!({ "newTicketsIds": [] })))
}

async fn count_open_by_reason(state: &AppState, total_key: &str) -> ApiResult<Value> {
    let rows: Vec<(String, i64)> = sqlx::query_as(
        "SELECT reason, count(*) FROM tickets WHERE resolution IS NULL GROUP BY reason",
    )
    .fetch_all(&state.pool)
    .await?;

    // JS Map: fixed keys start at 0, SQL rows override them and unknown
    // reasons are added; the total sums every entry.
    let mut counts: Vec<(String, i64)> =
        REPORT_REASONS.iter().map(|r| (r.to_string(), 0)).collect();
    for (reason, n) in rows {
        match counts.iter_mut().find(|(k, _)| *k == reason) {
            Some(slot) => slot.1 = n,
            None => counts.push((reason, n)),
        }
    }
    let total: i64 = counts.iter().map(|c| c.1).sum();

    let mut obj = serde_json::Map::new();
    for (key, value) in counts {
        obj.insert(key, json!(value));
    }
    obj.insert(total_key.to_string(), json!(total));
    Ok(Value::Object(obj))
}

async fn count_all_tickets(
    State(state): State<AppState>,
    auth: AuthUser,
) -> ApiResult<Json<Value>> {
    require_admin(&state, auth.id).await?;
    Ok(Json(
        count_open_by_reason(&state, "total_open_tickets").await?,
    ))
}

async fn count_all_reports(
    State(state): State<AppState>,
    auth: AuthUser,
) -> ApiResult<Json<Value>> {
    require_admin(&state, auth.id).await?;
    Ok(Json(
        count_open_by_reason(&state, "total_open_reports").await?,
    ))
}

// ---------------------------------------------------------------------------
// Alliances
// ---------------------------------------------------------------------------

async fn get_alliance(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(raw_id): Path<String>,
) -> ApiResult<Response> {
    alliance_detail(state, auth, raw_id, false).await
}

async fn get_alliance_full(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(raw_id): Path<String>,
) -> ApiResult<Response> {
    alliance_detail(state, auth, raw_id, true).await
}

async fn alliance_detail(
    state: AppState,
    auth: AuthUser,
    raw_id: String,
    full: bool,
) -> ApiResult<Response> {
    require_admin(&state, auth.id).await?;

    let Some(id) = js_parse_int(&raw_id).filter(|v| *v > 0.0) else {
        return Err(ApiError::bad_request("Bad Request"));
    };
    let Ok(id) = i32::try_from(id as i64) else {
        // Out of int4 range: no such alliance row can exist.
        return Ok(not_found_alliance());
    };

    let Some((id, name, description, hq_lat, hq_lon, pixels, created_at, updated_at)) =
        sqlx::query_as::<
            _,
            (
                i32,
                String,
                Option<String>,
                Option<f64>,
                Option<f64>,
                i32,
                DateTime<Utc>,
                DateTime<Utc>,
            ),
        >(
            "SELECT id, name, description, hq_latitude, hq_longitude, pixels_painted, \
                    created_at, updated_at FROM alliances WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
    else {
        return Ok(not_found_alliance());
    };

    if !full {
        // JS spreads the selected columns (id/name/pixelsPainted) plus
        // membersCount: 0; ownerId/ownerName are undefined and omitted.
        return Ok(Json(json!({
            "id": id,
            "name": name,
            "pixelsPainted": pixels,
            "membersCount": 0,
        }))
        .into_response());
    }

    let members: Vec<(
        i32,
        String,
        Option<String>,
        Option<String>,
        String,
        bool,
        String,
        Option<String>,
        i32,
    )> = sqlx::query_as(
        "SELECT id, name, nickname, discord, country, banned, role, picture, pixels_painted \
             FROM users WHERE alliance_id = $1 ORDER BY id",
    )
    .bind(id)
    .fetch_all(&state.pool)
    .await?;

    let members_json: Vec<Value> = members
        .iter()
        .map(
            |(id, name, nickname, discord, country, banned, role, picture, pixels_painted)| {
                json!({
                    "id": id,
                    "name": name,
                    "nickname": nickname,
                    "discord": discord,
                    "country": country,
                    "banned": banned,
                    "role": role,
                    "picture": picture,
                    "pixelsPainted": pixels_painted,
                })
            },
        )
        .collect();

    // JS owner = members[0] (first member).
    let owner = members.first();
    let owner_id = owner.map(|(id, ..)| *id);
    let owner_name = owner.map(|(_, name, nickname, ..)| display_name(nickname, name));

    let banned_users: Vec<(i32, String, Option<String>)> = sqlx::query_as(
        "SELECT u.id, u.name, u.nickname FROM alliance_banned_users b \
         JOIN users u ON u.id = b.user_id WHERE b.alliance_id = $1 ORDER BY u.id",
    )
    .bind(id)
    .fetch_all(&state.pool)
    .await?;
    let banned_json: Vec<Value> = banned_users
        .iter()
        .map(|(id, name, nickname)| json!({ "id": id, "name": name, "nickname": nickname }))
        .collect();

    Ok(Json(json!({
        "id": id,
        "name": name,
        "description": description.unwrap_or_default(),
        "hqLatitude": hq_lat,
        "hqLongitude": hq_lon,
        "pixelsPainted": pixels,
        "membersCount": members_json.len(),
        "members": members_json,
        "bannedUsers": banned_json,
        "createdAt": iso_ms(created_at),
        "updatedAt": iso_ms(updated_at),
        "ownerId": owner_id,
        "ownerName": owner_name,
    }))
    .into_response())
}

fn not_found_alliance() -> Response {
    (
        StatusCode::NOT_FOUND,
        Json(json!({ "error": "Alliance not found", "status": 404 })),
    )
        .into_response()
}

async fn search_alliances(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(params): Query<HashMap<String, String>>,
) -> ApiResult<Json<Value>> {
    require_admin(&state, auth.id).await?;
    let q = params.get("q").map(String::as_str).unwrap_or("");
    let q_id = js_parse_int(q).filter(|v| *v != 0.0).map(|v| v as i64);

    let rows: Vec<(i32, String, i32)> = sqlx::query_as(
        "SELECT id, name, pixels_painted FROM alliances \
         WHERE name ILIKE '%' || $1 || '%' OR ($2::bigint IS NOT NULL AND id = $2) \
         ORDER BY created_at DESC LIMIT 20",
    )
    .bind(q)
    .bind(q_id)
    .fetch_all(&state.pool)
    .await?;

    let results: Vec<Value> = rows
        .iter()
        .map(|(id, name, pixels)| json!({ "id": id, "name": name, "pixelsPainted": pixels }))
        .collect();
    Ok(Json(json!({ "results": results })))
}

// ---------------------------------------------------------------------------
// POST /admin/remove-ban | /admin/remove-timeout
// ---------------------------------------------------------------------------

async fn remove_ban(
    State(state): State<AppState>,
    auth: AuthUser,
    FlexibleJson(body): FlexibleJson<Value>,
) -> ApiResult<Json<Value>> {
    require_admin(&state, auth.id).await?;
    let user_id = json_parse_int(body.get("userId")).unwrap_or(0);
    let Ok(user_id) = i32::try_from(user_id) else {
        return Err(ApiError::bad_request("Bad Request"));
    };
    if user_id <= 0 {
        return Err(ApiError::bad_request("Bad Request"));
    }
    user_ops::ban_user(&state, user_id, false, None).await?;
    Ok(Json(json!({})))
}

async fn remove_timeout(
    State(state): State<AppState>,
    auth: AuthUser,
    FlexibleJson(body): FlexibleJson<Value>,
) -> ApiResult<Json<Value>> {
    require_admin(&state, auth.id).await?;
    let user_id = json_parse_int(body.get("userId")).unwrap_or(0);
    let Ok(user_id) = i32::try_from(user_id) else {
        return Err(ApiError::bad_request("Bad Request"));
    };
    if user_id <= 0 {
        return Err(ApiError::bad_request("Bad Request"));
    }
    user_ops::remove_timeout(&state, user_id).await?;
    Ok(Json(json!({})))
}

// ---------------------------------------------------------------------------
// GET /admin — admin.html
// ---------------------------------------------------------------------------

async fn admin_html(State(state): State<AppState>, auth: AuthUser) -> Response {
    if let Err(err) = require_admin(&state, auth.id).await {
        return err.into_response();
    }
    match tokio::fs::read_to_string("./frontend/admin.html").await {
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
// GET /admin/event/status — the newer admin UI probes for running events.
// Event mode is not implemented in this backend yet; report "no events" so
// the dashboard renders its idle state.
// ---------------------------------------------------------------------------

async fn event_status(State(state): State<AppState>, auth: AuthUser) -> ApiResult<Json<Value>> {
    require_admin(&state, auth.id).await?;
    Ok(Json(json!({ "events": [] })))
}

// ---------------------------------------------------------------------------
// Runtime settings — GET/POST /admin/settings (admin-editable, no restart)
// ---------------------------------------------------------------------------

async fn get_settings(State(state): State<AppState>, auth: AuthUser) -> ApiResult<Json<Value>> {
    require_admin(&state, auth.id).await?;
    let snapshot = state.settings_snapshot();
    let value = serde_json::to_value(&snapshot).map_err(|_| ApiError::Internal)?;
    Ok(Json(value))
}

async fn update_settings(
    State(state): State<AppState>,
    auth: AuthUser,
    FlexibleJson(body): FlexibleJson<Value>,
) -> ApiResult<Json<Value>> {
    require_admin(&state, auth.id).await?;
    let validated: RuntimeSettings = settings::validate(&body).map_err(ApiError::bad_request)?;
    settings::save(&state.pool, &validated).await?;
    *state.settings.write().unwrap() = validated.clone();
    let value = serde_json::to_value(&validated).map_err(|_| ApiError::Internal)?;
    Ok(Json(value))
}

// ---------------------------------------------------------------------------
// GET /admin/customize — self-contained dark customization page
// ---------------------------------------------------------------------------

const ADMIN_CUSTOMIZE_HTML: &str = include_str!("../admin_customize.html");

async fn customize_html(State(state): State<AppState>, auth: AuthUser) -> Response {
    if let Err(err) = require_admin(&state, auth.id).await {
        return err.into_response();
    }
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "text/html; charset=utf-8"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        ADMIN_CUSTOMIZE_HTML,
    )
        .into_response()
}
