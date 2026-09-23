// Fingerprint + proof-of-work endpoints (anti-automation collector):
//   POST /fp/collect     — traits submission (auth optional)
//   GET  /fp/challenge   — mint a PoW challenge (auth required)
//   POST /fp/challenge   — solve a PoW challenge (auth required)
//   GET  /fp.js          — the collector script (no auth)
use axum::extract::State;
use axum::http::header;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{json, Value};

use crate::auth::{AuthUser, MaybeAuthUser};
use crate::error::{ApiError, ApiResult};
use crate::extract::FlexibleJson;
use crate::services::antibot;
use crate::state::AppState;

const FP_JS: &str = include_str!("../fp.js");
/// ua_family / platform columns are coarse — hard cap their length.
const MAX_LABEL_LEN: usize = 120;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/fp/collect", post(fp_collect))
        .route("/fp/challenge", get(challenge_get).post(challenge_post))
        .route("/fp.js", get(fp_js))
}

// ---------------------------------------------------------------------------
// POST /fp/collect
// ---------------------------------------------------------------------------

async fn fp_collect(
    State(state): State<AppState>,
    auth: MaybeAuthUser,
    FlexibleJson(body): FlexibleJson<Value>,
) -> ApiResult<Json<Value>> {
    let settings = state.settings_snapshot();
    if settings.anti_bot_mode == "off" {
        return Ok(Json(json!({})));
    }

    let traits = body.get("traits").cloned().unwrap_or(Value::Null);
    let canonical = antibot::canonical_traits(&traits);
    let key = state.config.antibot_key();
    let visitor = antibot::visitor_id(&key, &canonical);
    let traits_hash = antibot::sha256_hex(&canonical);

    let ua = traits.get("ua").and_then(Value::as_str).unwrap_or_default();
    let platform = traits
        .get("platform")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let headless = traits.get("webdriver").and_then(Value::as_str) == Some("true")
        || traits.get("headless").and_then(Value::as_str) == Some("true")
        || antibot::is_headless_ua(ua);

    sqlx::query(
        "INSERT INTO fingerprints (visitor_id, traits_hash, ua_family, platform, headless) \
         VALUES ($1, $2, $3, $4, $5) \
         ON CONFLICT (visitor_id) DO UPDATE SET last_seen = now()",
    )
    .bind(&visitor)
    .bind(&traits_hash)
    .bind(truncate(ua, MAX_LABEL_LEN))
    .bind(truncate(platform, MAX_LABEL_LEN))
    .bind(headless)
    .execute(&state.pool)
    .await?;

    // Link the fingerprint to the signed-in account (paints untouched here).
    if let Some(user) = &auth.0 {
        sqlx::query(
            "INSERT INTO visitor_accounts (visitor_id, user_id) VALUES ($1, $2) \
             ON CONFLICT (visitor_id, user_id) DO UPDATE SET last_seen = now()",
        )
        .bind(&visitor)
        .bind(user.id)
        .execute(&state.pool)
        .await?;
    }

    // Already-suspicious visitors get a PoW challenge to work off.
    let mut challenge: Option<String> = None;
    if let Some(user) = &auth.0 {
        let score = antibot::score(&state, user.id).await;
        if score >= settings.anti_bot_enforce_threshold / 2 {
            challenge = Some(antibot::issue_challenge(&state.antibot, user.id));
        }
    }

    Ok(Json(json!({ "ok": true, "challenge": challenge })))
}

// ---------------------------------------------------------------------------
// GET/POST /fp/challenge
// ---------------------------------------------------------------------------

async fn challenge_get(State(state): State<AppState>, auth: AuthUser) -> Json<Value> {
    let challenge = antibot::issue_challenge(&state.antibot, auth.id);
    Json(json!({ "challenge": challenge }))
}

async fn challenge_post(
    State(state): State<AppState>,
    auth: AuthUser,
    FlexibleJson(body): FlexibleJson<Value>,
) -> ApiResult<Json<Value>> {
    let challenge = body
        .get("challenge")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let nonce = body
        .get("nonce")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if challenge.is_empty() || nonce.is_empty() {
        return Err(ApiError::bad_request("Bad Request"));
    }
    // The challenge must exist, be unexpired and belong to this user.
    if state.antibot.challenge_owner(challenge) != Some(auth.id) {
        return Err(ApiError::bad_request("Bad Request"));
    }
    if !antibot::pow_valid(challenge, nonce, state.config.anti_bot_pow_bits) {
        return Err(ApiError::bad_request("Bad Request"));
    }
    state.antibot.take_challenge(challenge);
    antibot::solve_pow(&state, auth.id).await;
    Ok(Json(json!({ "solved": true })))
}

// ---------------------------------------------------------------------------
// GET /fp.js
// ---------------------------------------------------------------------------

async fn fp_js() -> Response {
    (
        [
            (
                header::CONTENT_TYPE,
                "application/javascript; charset=utf-8",
            ),
            (header::CACHE_CONTROL, "public, max-age=300"),
        ],
        FP_JS,
    )
        .into_response()
}

fn truncate(value: &str, max: usize) -> String {
    value.chars().take(max).collect()
}
