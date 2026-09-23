// Discord OAuth routes — port of src/routes/discord.ts.
//
//   GET  /discord/link       → redirect /login/discord (no auth)
//   GET  /discord/unlink     → redirect /login/discord (no auth)
//   GET  /discord/configured → { configured } | 503
//   POST /discord/auth-url   → { url } (one-time state stored in AppState)
//   GET  /discord/callback   → exchange + link → redirect
//   POST /discord/unlink     → { success, message }
use std::collections::HashMap;

use axum::extract::{Query, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use rand::RngCore;
use serde_json::{json, Value};

use crate::auth::AuthUser;
use crate::discord::{bot, oauth};
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/discord/link", get(link_redirect))
        // GET /discord/unlink is a plain redirect; POST performs the unlink.
        .route("/discord/unlink", get(unlink_redirect).post(unlink))
        .route("/discord/configured", get(configured))
        .route("/discord/auth-url", post(auth_url))
        .route("/discord/callback", get(callback))
}

async fn link_redirect() -> Redirect {
    Redirect::temporary("/login/discord")
}

async fn unlink_redirect() -> Redirect {
    Redirect::temporary("/login/discord")
}

fn not_configured() -> Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(json!({ "error": "Discord OAuth is not configured", "status": 503 })),
    )
        .into_response()
}

/// 400 HTML pages — same markup as the JS backend's res.send() strings.
fn html_error(message: &str) -> Response {
    (
        StatusCode::BAD_REQUEST,
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        format!("<h1>Error: {message}</h1><a href=\"/auth/discord\">Try again</a>"),
    )
        .into_response()
}

fn error_redirect(message: &str) -> Redirect {
    Redirect::temporary(&format!(
        "/login/discord?error={}",
        oauth::percent_encode(message)
    ))
}

async fn configured(_auth: AuthUser, State(state): State<AppState>) -> Response {
    if !oauth::is_configured(&state.config) {
        return not_configured();
    }
    Json(json!({ "configured": true })).into_response()
}

async fn auth_url(auth: AuthUser, State(state): State<AppState>) -> ApiResult<Response> {
    if !oauth::is_configured(&state.config) {
        return Ok(not_configured());
    }

    let row: Option<(Option<String>,)> =
        sqlx::query_as("SELECT discord_user_id FROM users WHERE id = $1")
            .bind(auth.id)
            .fetch_optional(&state.pool)
            .await?;
    if row.and_then(|r| r.0).is_some() {
        return Err(ApiError::bad_request("Discord account is already linked"));
    }

    // 32 random bytes, hex-encoded (JS: crypto.randomBytes(32).toString("hex")).
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    let state_key: String = bytes.iter().map(|b| format!("{b:02X}")).collect();

    state.discord_links.insert(state_key.clone(), auth.id);

    Ok(Json(json!({ "url": oauth::authorize_url(&state.config, &state_key) })).into_response())
}

async fn callback(
    auth: AuthUser,
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let Some(code) = params.get("code").filter(|c| !c.is_empty()) else {
        return html_error("Missing authorization code");
    };
    let Some(state_str) = params.get("state").filter(|s| !s.is_empty()) else {
        return html_error("Missing state parameter");
    };
    // One-shot consume (expiry + ownership are checked inside take()).
    if !state.discord_links.take(state_str, auth.id) {
        return html_error("Invalid or expired state token");
    }

    let discord_user = match oauth::exchange_code(&state.http, &state.config, code).await {
        Ok(user) => user,
        Err(err) => return error_redirect(&err).into_response(),
    };
    if let Err(err) = oauth::link(
        &state.pool,
        &state.users,
        auth.id,
        &discord_user.id,
        &discord_user.username,
    )
    .await
    {
        return error_redirect(&err).into_response();
    }
    // Mirror of discordBot.updateUserId(discordUser.id): refresh the cooldown
    // from the member's current roles. Best-effort.
    if let Err(err) = bot::refresh_user_roles(&state, &discord_user.id).await {
        eprintln!("[Discord Bot] Error checking user roles: {err}");
    }

    Redirect::temporary("/login/discord").into_response()
}

async fn unlink(auth: AuthUser, State(state): State<AppState>) -> ApiResult<Json<Value>> {
    let row: Option<(Option<String>,)> =
        sqlx::query_as("SELECT discord_user_id FROM users WHERE id = $1")
            .bind(auth.id)
            .fetch_optional(&state.pool)
            .await?;
    if row.and_then(|r| r.0).is_none() {
        return Err(ApiError::bad_request("Discord account is not linked"));
    }

    sqlx::query(
        "UPDATE users SET discord = NULL, discord_user_id = NULL, charges_cooldown_ms = $2 WHERE id = $1",
    )
    .bind(auth.id)
    .bind(state.config.cooldown_ms)
    .execute(&state.pool)
    .await?;
    state.users.invalidate(auth.id);
    println!(
        "[Discord Bot] user#{} unlinked - cooldown reset to {}ms",
        auth.id, state.config.cooldown_ms
    );

    Ok(Json(json!({
        "success": true,
        "message": "Discord account unlinked"
    })))
}
