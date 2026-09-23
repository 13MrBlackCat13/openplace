// Port of src/routes/auth.ts (login / register / logout / password reset),
// backed by the AuthService behaviour in src/services/auth.ts.
use axum::extract::State;
use axum::http::request::Parts;
use axum::http::{header::SET_COOKIE, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use chrono::{Duration, SecondsFormat, Utc};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::auth::{clear_cookie_value, start_session, AuthUser};
use crate::error::{ApiError, ApiResult};
use crate::extract::{client_ip, FlexibleJson};
use crate::services::user_ops::get_ban;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/login", post(login))
        .route("/register", post(register))
        .route("/auth/logout", post(logout))
        .route("/auth/request-password-reset", post(request_password_reset))
        .route("/auth/reset-password", post(reset_password))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn iso_now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

/// Body field as a non-empty string (JS truthiness: `if (!username)` also
/// rejects `""`; the task contract additionally rejects non-strings).
fn str_field<'a>(body: &'a Value, key: &str) -> Option<&'a str> {
    body.get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
}

/// Success JSON with an auth cookie attached (JS: `res.setHeader("Set-Cookie", …)`).
fn cookie_response(cookie: &str) -> ApiResult<Response> {
    let mut response = Json(json!({ "success": true })).into_response();
    let value = HeaderValue::from_str(cookie).map_err(|_| ApiError::Internal)?;
    response.headers_mut().insert(SET_COOKIE, value);
    Ok(response)
}

/// 429 with a body that has no `status` field (JS `res.status(429).json({error})`).
fn too_many(msg: &str) -> Response {
    (StatusCode::TOO_MANY_REQUESTS, Json(json!({ "error": msg }))).into_response()
}

/// Raw JSON error without the `status` field (JS `res.status(x).json({error})`).
/// Used for the 401 login paths, which have no matching ApiError variant.
fn raw_error(status: StatusCode, msg: &str) -> Response {
    (status, Json(json!({ "error": msg }))).into_response()
}

/// JS: `rateLimiter.checkRateLimit(ip, …)` — only active when ENABLE_RATE_LIMIT.
/// Returns true when the request must be denied.
fn rate_limited(state: &AppState, ip: &str, attempts: i64, window_ms: i64) -> bool {
    state.config.enable_rate_limit && state.limiter.check(ip, attempts, window_ms).is_some()
}

/// JS: AuthService.messageForBanReason — BanReason → human-readable text.
fn ban_reason_message(reason: &str) -> &'static str {
    match reason {
        "inappropriate-content" => "Inappropriate content",
        "hate-speech" => "Hate speech",
        "doxxing" => "Doxxing",
        "bot" => "Botting",
        "griefing" => "Griefing",
        "multi-accounting" => "Multi-accounting",
        "ip-list" => "VPNs are not permitted",
        _ => "Other", // BanReason.Other and unknown values
    }
}

/// Columns read on the login path.
#[derive(sqlx::FromRow)]
struct LoginRow {
    id: i32,
    name: String,
    password_hash: String,
    banned: bool,
    suspension_reason: Option<String>,
    role: String,
}

// ---------------------------------------------------------------------------
// POST /login
// ---------------------------------------------------------------------------

async fn login(
    parts: Parts,
    State(state): State<AppState>,
    FlexibleJson(body): FlexibleJson<Value>,
) -> ApiResult<Response> {
    let ip = client_ip(&parts);
    let config = &state.config;

    let (Some(username), Some(password)) =
        (str_field(&body, "username"), str_field(&body, "password"))
    else {
        return Err(ApiError::bad_request("Username and password required"));
    };

    if rate_limited(
        &state,
        &ip,
        config.login_rate_limit_attempts,
        config.login_rate_limit_ms,
    ) {
        eprintln!("[{}] Rate limit exceeded for IP {ip}", iso_now());
        return Ok(too_many("Too many attempts. Please try again later."));
    }

    if !crate::utils::profanity::is_acceptable_username(username) {
        return Ok(raw_error(
            StatusCode::UNAUTHORIZED,
            "Invalid username or password or username contains offensive words",
        ));
    }

    let user: Option<LoginRow> = sqlx::query_as(
        "SELECT id, name, password_hash, banned, suspension_reason, role \
         FROM users WHERE name = $1",
    )
    .bind(username)
    .fetch_optional(&state.pool)
    .await?;

    let Some(user) = user else {
        return Ok(raw_error(
            StatusCode::UNAUTHORIZED,
            "Invalid username or password",
        ));
    };

    let password_valid = bcrypt::verify(password, &user.password_hash).unwrap_or(false);
    if !password_valid {
        return Ok(raw_error(
            StatusCode::UNAUTHORIZED,
            "Invalid username or password",
        ));
    }

    if user.role == "deleted" {
        return Err(ApiError::forbidden(
            "Your account has been deleted. If you believe this is a mistake, please contact the admin for assistance.",
        ));
    }

    if user.banned {
        let reason = ban_reason_message(user.suspension_reason.as_deref().unwrap_or("other"));
        return Err(ApiError::forbidden(format!(
            "You have been banned. Reason: {reason}"
        )));
    }

    // JS: userService.setLastIP (prisma.user.update bumps @updatedAt).
    sqlx::query("UPDATE users SET last_ip = $2, updated_at = now() WHERE id = $1")
        .bind(user.id)
        .bind(&ip)
        .execute(&state.pool)
        .await?;
    state.users.invalidate(user.id);

    let (_session_id, cookie) = start_session(&state, user.id).await?;
    state.limiter.record_success(&ip);
    eprintln!("[{}] [{ip}] {}#{} logged in", iso_now(), user.name, user.id);
    cookie_response(&cookie)
}

// ---------------------------------------------------------------------------
// POST /register
// ---------------------------------------------------------------------------

async fn register(
    parts: Parts,
    State(state): State<AppState>,
    FlexibleJson(body): FlexibleJson<Value>,
) -> ApiResult<Response> {
    let ip = client_ip(&parts);
    let config = &state.config;

    let (Some(username), Some(password)) =
        (str_field(&body, "username"), str_field(&body, "password"))
    else {
        return Err(ApiError::bad_request("Username and password required"));
    };

    if rate_limited(
        &state,
        &ip,
        config.signup_rate_limit_attempts,
        config.signup_rate_limit_ms,
    ) {
        eprintln!("[{}] Rate limit exceeded for IP {ip}", iso_now());
        return Ok(too_many("Too many attempts. Please try again later."));
    }

    if !crate::utils::sanitize::is_valid_username(username) {
        return Err(ApiError::bad_request(
            "Username must be between 3 and 16 characters and cannot contain special characters.",
        ));
    }

    let exists: Option<(i32,)> = sqlx::query_as("SELECT id FROM users WHERE name = $1")
        .bind(username)
        .fetch_optional(&state.pool)
        .await?;
    if exists.is_some() {
        return Err(ApiError::bad_request("Username already exists"));
    }

    let header_country = parts
        .headers
        .get("cf-ipcountry")
        .and_then(|v| v.to_str().ok());
    // JS: (/^[A-Z]{2}$/).test(country) ? country : "US". "T1" (Tor) passes the
    // regex on purpose so that getBan can enforce BLOCK_TOR — normalize_country
    // would map it to "US" and disable that check, hence the inline validation.
    let country = match header_country {
        Some(c) if c.len() == 2 && c.chars().all(|ch| ch.is_ascii_uppercase()) => c.to_string(),
        _ => "US".to_string(),
    };

    if let Some(ban) = get_ban(&state.pool, config, &ip, &country).await? {
        eprintln!("Banned IP {ip} attempted to register as {username}");
        let reason = ban_reason_message(&ban.reason);
        return Err(ApiError::forbidden(format!(
            "You have been banned. Reason: {reason}"
        )));
    }

    let password_hash = bcrypt::hash(password, 10).map_err(|_| ApiError::Internal)?;
    let (count,): (i64,) = sqlx::query_as("SELECT count(*) FROM users")
        .fetch_one(&state.pool)
        .await?;
    let role = if count == 0 { "admin" } else { "user" };

    let (user_id, user_name): (i32, String) = sqlx::query_as(
        r#"
        INSERT INTO users (
            name, nickname, password_hash, registration_ip, last_ip, country, role,
            droplets, current_charges, max_charges, charges_cooldown_ms,
            pixels_painted, level, extra_colors_bitmap, equipped_flag, charges_last_updated_at
        ) VALUES ($1, $1, $2, $3, $3, $4, $5, 1000, 20, 20, $6, 0, 1, 0, 0, now())
        RETURNING id, name
        "#,
    )
    .bind(username)
    .bind(&password_hash)
    .bind(&ip)
    .bind(&country)
    .bind(role)
    .bind(i32::try_from(config.cooldown_ms).unwrap_or(i32::MAX))
    .fetch_one(&state.pool)
    .await?;

    let (_session_id, cookie) = start_session(&state, user_id).await?;
    state.limiter.record_success(&ip);
    eprintln!(
        "[{}] [{ip}] registered with {user_name}#{user_id}!",
        iso_now()
    );
    cookie_response(&cookie)
}

// ---------------------------------------------------------------------------
// POST /auth/logout
// ---------------------------------------------------------------------------

async fn logout(
    parts: Parts,
    State(state): State<AppState>,
    auth: AuthUser,
) -> ApiResult<Response> {
    if let Ok(session_id) = Uuid::parse_str(&auth.session_id) {
        sqlx::query("DELETE FROM sessions WHERE id = $1")
            .bind(session_id)
            .execute(&state.pool)
            .await?;
        state.sessions.remove(session_id);
    }
    let name = state
        .users
        .get_or_load(&state.pool, auth.id)
        .await
        .map(|u| u.display_name().to_string())
        .unwrap_or_else(|| format!("user:{}", auth.id));
    let ip = client_ip(&parts);
    eprintln!("[{}] [{ip}] {name}#{} logged out", iso_now(), auth.id);
    cookie_response(&clear_cookie_value())
}

// ---------------------------------------------------------------------------
// POST /auth/request-password-reset
// ---------------------------------------------------------------------------

async fn request_password_reset(
    parts: Parts,
    State(state): State<AppState>,
    FlexibleJson(body): FlexibleJson<Value>,
) -> ApiResult<Response> {
    let ip = client_ip(&parts);
    let config = &state.config;

    let Some(username) = str_field(&body, "username") else {
        return Err(ApiError::bad_request("Username required"));
    };

    if rate_limited(
        &state,
        &ip,
        config.password_reset_rate_limit_attempts,
        config.password_reset_rate_limit_ms,
    ) {
        return Ok(too_many(
            "Too many password reset attempts. Please try again later.",
        ));
    }

    let user: Option<(i32, String, Option<String>, bool, String)> =
        sqlx::query_as("SELECT id, name, discord_user_id, banned, role FROM users WHERE name = $1")
            .bind(username)
            .fetch_optional(&state.pool)
            .await?;

    // Do not reveal whether the account exists.
    let Some((user_id, user_name, discord_user_id, banned, role)) = user else {
        state.limiter.record_success(&ip);
        return Ok(Json(json!({ "success": true })).into_response());
    };

    if role == "deleted" {
        return Err(ApiError::forbidden(
            "Your account has been deleted. If you believe this is a mistake, please contact the admin for assistance.",
        ));
    }
    if banned {
        return Err(ApiError::forbidden("You have been banned."));
    }
    let Some(discord_user_id) = discord_user_id else {
        return Err(ApiError::bad_request(
            "Your account could not be recovered as it was not linked with a Discord account. Please contact an administrator for assistance.",
        ));
    };

    let recent: Option<(chrono::DateTime<Utc>,)> = sqlx::query_as(
        "SELECT created_at FROM password_reset_tokens \
         WHERE user_id = $1 AND created_at >= now() - interval '10 minutes' LIMIT 1",
    )
    .bind(user_id)
    .fetch_optional(&state.pool)
    .await?;
    if recent.is_some() {
        return Ok(too_many(
            "You already requested a password reset recently. Please wait before sending another one.",
        ));
    }

    sqlx::query("DELETE FROM password_reset_tokens WHERE user_id = $1")
        .bind(user_id)
        .execute(&state.pool)
        .await?;

    let token = Uuid::new_v4();
    let expires_at = Utc::now() + Duration::hours(1);
    sqlx::query("INSERT INTO password_reset_tokens (id, user_id, expires_at) VALUES ($1, $2, $3)")
        .bind(token)
        .bind(user_id)
        .bind(expires_at)
        .execute(&state.pool)
        .await?;

    let link = format!("{}/login/set-password?token={token}", config.external_url);
    let message = format!(
        "### Password Reset Requested\n\n\
         A password reset was requested for your openplace account. If you requested this, click this link to reset your password:\n\n\
         [Reset your password]({link})\n\n\
         *Wasn't you? You can safely ignore this message.*"
    );

    send_dm(&state, &discord_user_id, &message).await;

    state.limiter.record_success(&ip);
    eprintln!(
        "[{}] [{ip}] Password reset requested for {user_name}#{user_id}",
        iso_now()
    );
    Ok(Json(json!({ "success": true })).into_response())
}

// ---------------------------------------------------------------------------
// POST /auth/reset-password
// ---------------------------------------------------------------------------

async fn reset_password(
    parts: Parts,
    State(state): State<AppState>,
    FlexibleJson(body): FlexibleJson<Value>,
) -> ApiResult<Response> {
    let ip = client_ip(&parts);
    let config = &state.config;

    let (Some(token), Some(password)) = (str_field(&body, "token"), str_field(&body, "password"))
    else {
        return Err(ApiError::bad_request("Token and password required"));
    };
    if password.len() < 8 {
        return Err(ApiError::bad_request(
            "Password must be at least 8 characters long",
        ));
    }

    if rate_limited(
        &state,
        &ip,
        config.password_reset_rate_limit_attempts,
        config.password_reset_rate_limit_ms,
    ) {
        return Ok(too_many(
            "Too many password reset attempts. Please try again later.",
        ));
    }

    // findUnique({ id }) with a non-UUID would throw in JS; treat it as an
    // invalid token (same 400 the client already handles).
    let Ok(token_id) = Uuid::parse_str(token) else {
        return Ok(raw_error(
            StatusCode::BAD_REQUEST,
            "Invalid or expired reset token",
        ));
    };

    let reset_token: Option<(i32, chrono::DateTime<Utc>)> =
        sqlx::query_as("SELECT user_id, expires_at FROM password_reset_tokens WHERE id = $1")
            .bind(token_id)
            .fetch_optional(&state.pool)
            .await?;
    let Some((user_id, _expires_at)) = reset_token.filter(|(_, exp)| *exp >= Utc::now()) else {
        return Ok(raw_error(
            StatusCode::BAD_REQUEST,
            "Invalid or expired reset token",
        ));
    };

    let user: Option<(i32, String, bool, String)> =
        sqlx::query_as("SELECT id, name, banned, role FROM users WHERE id = $1")
            .bind(user_id)
            .fetch_optional(&state.pool)
            .await?;
    let Some((_id, user_name, banned, role)) = user else {
        return Err(ApiError::bad_request("User not found"));
    };
    if role == "deleted" {
        return Err(ApiError::forbidden(
            "Your account has been deleted. If you believe this is a mistake, please contact the admin for assistance.",
        ));
    }
    if banned {
        return Err(ApiError::forbidden("You have been banned."));
    }

    let password_hash = bcrypt::hash(password, 10).map_err(|_| ApiError::Internal)?;
    let mut tx = state.pool.begin().await?;
    sqlx::query("UPDATE users SET password_hash = $2, updated_at = now() WHERE id = $1")
        .bind(user_id)
        .bind(&password_hash)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM password_reset_tokens WHERE id = $1")
        .bind(token_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM sessions WHERE user_id = $1")
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    state.sessions.remove_all_for_user(user_id);

    state.limiter.record_success(&ip);
    eprintln!(
        "[{}] [{ip}] Password reset completed for {user_name}#{user_id}",
        iso_now()
    );
    Ok(Json(json!({ "success": true })).into_response())
}

// ---------------------------------------------------------------------------
// Discord DM
// ---------------------------------------------------------------------------

/// Port of DiscordBot.sendDM (src/discord/bot.ts): opens a DM channel through
/// the Discord REST API and posts the markdown message as an embed with the
/// same author/color. No-ops when no bot token is configured; failures are
/// logged and swallowed (the JS promise is awaited but never propagates).
///
/// Note: `crate::discord::bot` is still a placeholder without `send_dm`, so
/// the REST call lives here for now; once the discord module lands a
/// `send_dm(&AppState, …)` this helper can be swapped for it.
async fn send_dm(state: &AppState, discord_user_id: &str, message: &str) {
    let Some(token) = state
        .config
        .discord_bot_token
        .as_deref()
        .filter(|t| !t.is_empty())
    else {
        return;
    };
    let auth = format!("Bot {token}");
    let base = "https://discord.com/api/v10";
    let client = &state.http;

    let channel_id: Option<String> = match client
        .post(format!("{base}/users/@me/channels"))
        .header("Authorization", &auth)
        .json(&json!({ "recipient_id": discord_user_id }))
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => match resp.json::<Value>().await {
            Ok(v) => v.get("id").and_then(Value::as_str).map(str::to_string),
            Err(_) => None,
        },
        _ => None,
    };
    let Some(channel_id) = channel_id else {
        eprintln!("[Discord Bot] Error sending DM: failed to open DM channel");
        return;
    };

    let payload = json!({
        "embeds": [{
            "color": 0x0041_69E2,
            "author": {
                "name": "openplace",
                "icon_url": "https://openplace.live/img/favicon-96x96.png",
            },
            "description": message,
        }]
    });
    if let Err(err) = client
        .post(format!("{base}/channels/{channel_id}/messages"))
        .header("Authorization", &auth)
        .json(&payload)
        .send()
        .await
    {
        eprintln!("[Discord Bot] Error sending DM: {err}");
    }
}
