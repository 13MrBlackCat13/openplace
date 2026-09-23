use chrono::Utc;
use dashmap::DashMap;
use sqlx::PgPool;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::config::Config;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

/// In-memory map of Discord OAuth link flows: state → (user_id, created_at).
pub struct DiscordLinkStates(pub DashMap<String, (i32, Instant)>);

impl Default for DiscordLinkStates {
    fn default() -> Self {
        Self::new()
    }
}

impl DiscordLinkStates {
    pub fn new() -> Self {
        Self(DashMap::new())
    }

    pub fn insert(&self, state: String, user_id: i32) {
        self.0.insert(state, (user_id, Instant::now()));
    }

    /// One-shot consume: None if missing/expired/foreign user.
    pub fn take(&self, state: &str, user_id: i32) -> bool {
        const TTL: Duration = Duration::from_secs(600);
        match self.0.remove(state) {
            Some((_, (owner, at))) => owner == user_id && at.elapsed() < TTL,
            None => false,
        }
    }

    pub fn start_cleanup(self: Arc<Self>) {
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(600)).await;
                self.0
                    .retain(|_, (_, at)| at.elapsed() < Duration::from_secs(600));
            }
        });
    }
}

/// Result of an IP/country ban lookup (auth service `getBan`).
pub struct BanInfo {
    pub reason: String,
}

/// IP → (cidr, ipv4 number, ipv6 bytes) triple used by BannedIP queries.
pub fn ip_to_parts(ip: &str) -> (String, Option<i64>, Option<Vec<u8>>) {
    use std::net::IpAddr;
    match ip.parse::<IpAddr>() {
        Ok(IpAddr::V4(v4)) => {
            let n = u32::from(v4) as i64;
            (format!("{ip}/32"), Some(n), None)
        }
        Ok(IpAddr::V6(v6)) => {
            let segs = v6.segments();
            // IPv4-mapped IPv6 (::ffff:a.b.c.d) is normalized to its v4 form,
            // matching the JS cidr-tools behaviour for ban storage.
            if let Some(v4) = v6.to_ipv4_mapped() {
                let n = u32::from(v4) as i64;
                let v4str = v4.to_string();
                (format!("{v4str}/32"), Some(n), None)
            } else {
                let mut bytes = Vec::with_capacity(16);
                for s in segs {
                    bytes.extend_from_slice(&s.to_be_bytes());
                }
                (format!("{ip}/128"), None, Some(bytes))
            }
        }
        Err(_) => (format!("{ip}/32"), None, None),
    }
}

/// Checks Tor-country and BannedIP tables; mirrors AuthService.getBan.
pub async fn get_ban(
    pool: &PgPool,
    config: &Config,
    ip: &str,
    country: &str,
) -> ApiResult<Option<BanInfo>> {
    if config.block_tor && country.eq_ignore_ascii_case("T1") {
        return Ok(Some(BanInfo {
            reason: "ip-list".to_string(), // "VPNs are not permitted"
        }));
    }
    let (cidr, v4, v6) = ip_to_parts(ip);
    let row: Option<(String,)> = sqlx::query_as(
        r#"
        SELECT suspension_reason FROM banned_ips
        WHERE (
                (user_id IS NULL)
                OR (user_id IS NOT NULL AND cidr = $1)
              )
          AND (
                (ipv4_min IS NOT NULL AND $2 IS NOT NULL AND $2 >= ipv4_min AND $2 <= ipv4_max)
                OR (ipv6_min IS NOT NULL AND $3 IS NOT NULL AND $3 = ipv6_min AND $3 = ipv6_max)
              )
        LIMIT 1
        "#,
    )
    .bind(&cidr)
    .bind(v4)
    .bind(v6)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|(reason,)| BanInfo { reason }))
}

/// Bans (or unbans) a user and cascades the ban to every account sharing
/// their registration/last IP — port of userService.ban + authService.banUser.
pub async fn ban_user(
    state: &AppState,
    user_id: i32,
    banned: bool,
    reason: Option<String>,
) -> ApiResult<()> {
    let row: Option<(Option<String>, Option<String>)> =
        sqlx::query_as("SELECT registration_ip, last_ip FROM users WHERE id = $1")
            .bind(user_id)
            .fetch_optional(&state.pool)
            .await?;
    let Some((registration_ip, last_ip)) = row else {
        return Ok(());
    };

    sqlx::query(
        "UPDATE users SET banned = $2, suspension_reason = $3, updated_at = now() WHERE id = $1",
    )
    .bind(user_id)
    .bind(banned)
    .bind(reason.clone())
    .execute(&state.pool)
    .await?;
    state.users.invalidate(user_id);

    if !banned {
        // Unban: drop this user's BannedIP entries.
        sqlx::query("DELETE FROM banned_ips WHERE user_id = $1")
            .bind(user_id)
            .execute(&state.pool)
            .await?;
        return Ok(());
    }

    let reason = reason.unwrap_or_else(|| "other".to_string());
    for ip in [registration_ip, last_ip].into_iter().flatten() {
        let (cidr, v4, v6) = ip_to_parts(&ip);
        let exists: Option<(i32,)> = sqlx::query_as(
            "SELECT id FROM banned_ips WHERE cidr = $1 AND ipv4_min IS NOT DISTINCT FROM $2 AND ipv6_min IS NOT DISTINCT FROM $3 LIMIT 1",
        )
        .bind(&cidr)
        .bind(v4)
        .bind(v6.clone())
        .fetch_optional(&state.pool)
        .await?;
        if exists.is_none() {
            sqlx::query(
                "INSERT INTO banned_ips (cidr, ipv4_min, ipv4_max, ipv6_min, ipv6_max, suspension_reason, user_id) \
                 VALUES ($1, $2, $2, $3, $3, $4, $5)",
            )
            .bind(&cidr)
            .bind(v4)
            .bind(v6.clone())
            .bind(&reason)
            .bind(user_id)
            .execute(&state.pool)
            .await?;
        }

        // Cascade: ban every other non-banned account sharing this IP.
        let shared: Vec<(i32,)> = sqlx::query_as(
            "SELECT DISTINCT id FROM users \
             WHERE banned = false AND id <> $1 AND (registration_ip = $2 OR last_ip = $2)",
        )
        .bind(user_id)
        .bind(&ip)
        .fetch_all(&state.pool)
        .await?;
        for (other,) in shared {
            let _ = Box::pin(ban_user(state, other, true, Some(reason.clone()))).await;
        }
    }
    Ok(())
}

/// 3-day timeout — port of userService.timeout as used by ticket resolution.
pub async fn timeout_user(state: &AppState, user_id: i32, days: i64) -> ApiResult<()> {
    sqlx::query("UPDATE users SET timeout_until = now() + make_interval(days => $2), updated_at = now() WHERE id = $1")
        .bind(user_id)
        .bind(days)
        .execute(&state.pool)
        .await?;
    state.users.invalidate(user_id);
    Ok(())
}

pub async fn remove_timeout(state: &AppState, user_id: i32) -> ApiResult<()> {
    sqlx::query("UPDATE users SET timeout_until = now(), updated_at = now() WHERE id = $1")
        .bind(user_id)
        .execute(&state.pool)
        .await?;
    state.users.invalidate(user_id);
    Ok(())
}

/// Country header → country code defaulting to "US" (JS: ^[A-Z]{2}$ check).
pub fn normalize_country(header: Option<&str>) -> String {
    match header {
        Some(c) if c.len() == 2 && c.chars().all(|ch| ch.is_ascii_uppercase()) && c != "T1" => {
            c.to_string()
        }
        _ => "US".to_string(),
    }
}

/// Verifies a paint-time suspension and cascades a persistent ban when the
/// row says banned/timeout (JS: force-ban + Error("banned")).
pub async fn ensure_not_suspended(state: &AppState, user_id: i32) -> ApiResult<()> {
    let Some(user) = state.users.get_or_load(&state.pool, user_id).await else {
        return Err(ApiError::Refresh);
    };
    if user.banned {
        let _ = ban_user(state, user_id, true, user.suspension_reason.clone()).await;
        return Err(ApiError::ban(user.suspension_reason.clone()));
    }
    if user.timeout_until > Utc::now() {
        return Err(ApiError::timeout(user.suspension_reason.clone()));
    }
    Ok(())
}
