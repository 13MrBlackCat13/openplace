use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use chrono::{DateTime, Duration, Utc};
use dashmap::DashMap;
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::collections::HashSet;
use uuid::Uuid;

use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

pub const COOKIE_NAME: &str = "j";
pub const COOKIE_MAX_AGE_SECS: i64 = 2_592_000; // 30 days
pub const TOKEN_ISSUER: &str = "openplace";

#[derive(Debug, Serialize, Deserialize)]
#[allow(non_snake_case)]
pub struct AuthClaims {
    /// JS backend uses `userId` (camelCase) — kept for token compatibility.
    pub userId: i32,
    pub sessionId: String,
    pub role: Option<String>,
    pub iss: String,
    pub exp: i64,
    pub iat: i64,
}

pub fn create_token(
    secret: &str,
    user_id: i32,
    session_id: &str,
    expires_at: DateTime<Utc>,
) -> ApiResult<String> {
    let now = Utc::now();
    let claims = AuthClaims {
        userId: user_id,
        sessionId: session_id.to_string(),
        role: None,
        iss: TOKEN_ISSUER.to_string(),
        exp: expires_at.timestamp(),
        iat: now.timestamp(),
    };
    encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_err(|_| ApiError::Internal)
}

pub fn verify_token(secret: &str, token: &str) -> Option<AuthClaims> {
    let mut validation = Validation::new(Algorithm::HS256);
    validation.validate_exp = true;
    validation.validate_aud = false;
    validation.required_spec_claims.clear();
    let data = decode::<AuthClaims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )
    .ok()?;
    if data.claims.iss != TOKEN_ISSUER {
        return None;
    }
    Some(data.claims)
}

pub fn get_cookie(parts: &Parts, name: &str) -> Option<String> {
    let cookies = parts
        .headers
        .get(axum::http::header::COOKIE)?
        .to_str()
        .ok()?;
    for pair in cookies.split(';') {
        let pair = pair.trim();
        if let Some(rest) = pair.strip_prefix(name) {
            if let Some(value) = rest.strip_prefix('=') {
                return Some(value.to_string());
            }
        }
    }
    None
}

pub fn auth_cookie_value(token: &str) -> String {
    format!("{COOKIE_NAME}={token}; HttpOnly; Path=/; Max-Age={COOKIE_MAX_AGE_SECS}; SameSite=Lax")
}

pub fn clear_cookie_value() -> String {
    format!("{COOKIE_NAME}=; HttpOnly; Path=/; Max-Age=0; SameSite=Lax")
}

/// In-memory session validation cache. The JS backend hits the DB on every
/// authenticated request; here the DB is consulted at most once per TTL.
pub struct SessionCache {
    by_id: DashMap<Uuid, CachedSession>,
    by_user: DashMap<i32, HashSet<Uuid>>,
    ttl_ms: u64,
}

#[derive(Clone)]
struct CachedSession {
    user_id: i32,
    expires_at: DateTime<Utc>,
    cached_at: DateTime<Utc>,
}

impl SessionCache {
    pub fn new(ttl_ms: u64) -> Self {
        Self {
            by_id: DashMap::new(),
            by_user: DashMap::new(),
            ttl_ms,
        }
    }

    /// Cache-only validation (no I/O). Used on the hot auth path.
    pub fn validate_cached(&self, session_id: &str, claimed_user: i32) -> bool {
        let Ok(uuid) = Uuid::parse_str(session_id) else {
            return false;
        };
        let now = Utc::now();
        self.by_id.get(&uuid).is_some_and(|c| {
            (now - c.cached_at).num_milliseconds() < self.ttl_ms as i64
                && c.user_id == claimed_user
                && c.expires_at > now
        })
    }

    pub async fn validate_cached_or_db(
        &self,
        pool: &PgPool,
        session_id: &str,
        claimed_user: i32,
    ) -> bool {
        let Ok(uuid) = Uuid::parse_str(session_id) else {
            return false;
        };
        let now = Utc::now();
        if let Some(c) = self.by_id.get(&uuid) {
            if (now - c.cached_at).num_milliseconds() < self.ttl_ms as i64 {
                return c.user_id == claimed_user && c.expires_at > now;
            }
        }
        let row: Option<(i32, DateTime<Utc>)> =
            sqlx::query_as("SELECT user_id, expires_at FROM sessions WHERE id = $1")
                .bind(uuid)
                .fetch_optional(pool)
                .await
                .ok()
                .flatten();
        match row {
            Some((user_id, expires_at)) if user_id == claimed_user && expires_at > now => {
                self.insert(uuid, user_id, expires_at);
                true
            }
            _ => false,
        }
    }

    pub fn insert(&self, session_id: Uuid, user_id: i32, expires_at: DateTime<Utc>) {
        self.by_id.insert(
            session_id,
            CachedSession {
                user_id,
                expires_at,
                cached_at: Utc::now(),
            },
        );
        self.by_user.entry(user_id).or_default().insert(session_id);
    }

    pub fn remove(&self, session_id: Uuid) {
        if let Some((_, c)) = self.by_id.remove(&session_id) {
            if let Some(mut set) = self.by_user.get_mut(&c.user_id) {
                set.remove(&session_id);
            }
        }
    }

    pub fn remove_all_for_user(&self, user_id: i32) {
        if let Some((_, set)) = self.by_user.remove(&user_id) {
            for id in set {
                self.by_id.remove(&id);
            }
        }
    }
}

/// Creates a DB session + JWT; returns (session_id, Set-Cookie value).
pub async fn start_session(state: &AppState, user_id: i32) -> ApiResult<(Uuid, String)> {
    let session_id = Uuid::new_v4();
    let expires_at = Utc::now() + Duration::seconds(COOKIE_MAX_AGE_SECS);
    sqlx::query("INSERT INTO sessions (id, user_id, expires_at) VALUES ($1, $2, $3)")
        .bind(session_id)
        .bind(user_id)
        .bind(expires_at)
        .execute(&state.pool)
        .await?;
    state.sessions.insert(session_id, user_id, expires_at);
    let token = create_token(
        &state.config.jwt_secret,
        user_id,
        &session_id.to_string(),
        expires_at,
    )?;
    Ok((session_id, auth_cookie_value(&token)))
}

/// Fresh role check. The in-memory user cache is authoritative for roles
/// written through this process (single-process deployment, same as the JS
/// backend); the fallback TTL re-reads the DB otherwise.
pub async fn db_role(state: &AppState, user_id: i32) -> ApiResult<String> {
    if let Some(u) = state.users.peek(user_id) {
        return Ok(u.role.clone());
    }
    let role: Option<(String,)> = sqlx::query_as("SELECT role FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_optional(&state.pool)
        .await?;
    role.map(|r| r.0).ok_or(ApiError::UserNotFound)
}

pub async fn require_admin(state: &AppState, user_id: i32) -> ApiResult<()> {
    if db_role(state, user_id).await? != "admin" {
        return Err(ApiError::forbidden("Forbidden"));
    }
    Ok(())
}

pub async fn require_moderator(state: &AppState, user_id: i32) -> ApiResult<()> {
    if db_role(state, user_id).await? == "user" {
        return Err(ApiError::forbidden("Forbidden"));
    }
    Ok(())
}

/// Extractor for endpoints requiring authentication (JWT cookie + session).
#[derive(Debug, Clone)]
pub struct AuthUser {
    pub id: i32,
    pub session_id: String,
    pub role: String,
}

impl AuthUser {
    async fn from_parts(state: &AppState, parts: &Parts) -> Result<Self, ApiError> {
        let token = get_cookie(parts, COOKIE_NAME).ok_or(ApiError::Unauthorized)?;
        let claims =
            verify_token(&state.config.jwt_secret, &token).ok_or(ApiError::Unauthorized)?;
        let user_id = claims.userId;
        let session_id = claims.sessionId;
        if session_id.is_empty() {
            return Err(ApiError::Unauthorized);
        }
        if !state
            .sessions
            .validate_cached_or_db(&state.pool, &session_id, user_id)
            .await
        {
            return Err(ApiError::Unauthorized);
        }
        Ok(Self {
            id: user_id,
            session_id,
            role: claims.role.unwrap_or_else(|| "user".to_string()),
        })
    }

    pub fn id(&self) -> i32 {
        self.id
    }
}

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        Self::from_parts(state, parts).await
    }
}

/// Extractor for endpoints where auth is optional (errors are swallowed).
#[derive(Debug, Clone, Default)]
pub struct MaybeAuthUser(pub Option<AuthUser>);

impl FromRequestParts<AppState> for MaybeAuthUser {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        Ok(MaybeAuthUser(AuthUser::from_parts(state, parts).await.ok()))
    }
}
