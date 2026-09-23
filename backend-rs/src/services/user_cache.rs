use chrono::{DateTime, Utc};
use dashmap::DashMap;
use sqlx::FromRow;
use sqlx::PgPool;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Authoritative in-process snapshot of a user row, refreshed from SQL on
/// writes (`UPDATE ... RETURNING`). The JS backend re-reads the user on every
/// authenticated call; this removes that round trip.
#[derive(Debug, Clone, FromRow)]
pub struct UserRuntime {
    pub id: i32,
    pub name: String,
    pub nickname: Option<String>,
    pub country: String,
    pub banned: bool,
    pub verified: bool,
    pub suspension_reason: Option<String>,
    pub timeout_until: DateTime<Utc>,
    pub role: String,
    pub pixels_painted: i32,
    pub droplets: i32,
    pub max_charges: f64,
    pub current_charges: f64,
    pub charges_cooldown_ms: i32,
    pub charges_last_updated_at: DateTime<Utc>,
    pub extra_colors_bitmap: i32,
    pub flags_bitmap: Option<Vec<u8>>,
    pub equipped_flag: i32,
    pub show_last_pixel: bool,
    pub picture: Option<String>,
    pub level: f64,
    pub alliance_id: Option<i32>,
    pub alliance_role: String,
    pub discord: Option<String>,
    pub discord_user_id: Option<String>,
    pub is_customer: bool,
}

impl UserRuntime {
    pub const COLUMNS: &'static str = "id, name, nickname, country, banned, verified, \
        suspension_reason, timeout_until, role, pixels_painted, droplets, max_charges, \
        current_charges, charges_cooldown_ms, charges_last_updated_at, extra_colors_bitmap, \
        flags_bitmap, equipped_flag, show_last_pixel, picture, level, alliance_id, \
        alliance_role, discord, discord_user_id, is_customer";

    pub fn display_name(&self) -> &str {
        self.nickname
            .as_deref()
            .filter(|n| !n.is_empty())
            .unwrap_or(&self.name)
    }
}

pub struct UserCache {
    map: DashMap<i32, (Arc<UserRuntime>, Instant)>,
    ttl: Duration,
}

impl UserCache {
    pub fn new(ttl_ms: u64) -> Self {
        Self {
            map: DashMap::new(),
            ttl: Duration::from_millis(ttl_ms),
        }
    }

    pub fn peek(&self, user_id: i32) -> Option<Arc<UserRuntime>> {
        let (user, at) = self.map.get(&user_id)?.value().clone();
        (at.elapsed() < self.ttl).then_some(user)
    }

    pub async fn get_or_load(&self, pool: &PgPool, user_id: i32) -> Option<Arc<UserRuntime>> {
        if let Some(u) = self.peek(user_id) {
            return Some(u);
        }
        let user: Option<UserRuntime> = sqlx::query_as(&format!(
            "SELECT {} FROM users WHERE id = $1",
            UserRuntime::COLUMNS
        ))
        .bind(user_id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten();
        let user = user.map(Arc::new)?;
        self.map.insert(user_id, (user.clone(), Instant::now()));
        Some(user)
    }

    pub fn store(&self, user: Arc<UserRuntime>) {
        self.map.insert(user.id, (user, Instant::now()));
    }

    pub fn invalidate(&self, user_id: i32) {
        self.map.remove(&user_id);
    }

    /// Replace the cached entry from a fresh DB read (used after writes that
    /// did not go through UPDATE ... RETURNING).
    pub async fn refresh(&self, pool: &PgPool, user_id: i32) -> Option<Arc<UserRuntime>> {
        let user: Option<UserRuntime> = sqlx::query_as(&format!(
            "SELECT {} FROM users WHERE id = $1",
            UserRuntime::COLUMNS
        ))
        .bind(user_id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten();
        let user = user.map(Arc::new)?;
        self.map.insert(user_id, (user.clone(), Instant::now()));
        Some(user)
    }
}
