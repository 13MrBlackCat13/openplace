use std::env;

fn s(key: &str) -> Option<String> {
    env::var(key).ok().filter(|v| !v.is_empty())
}
fn s_or(key: &str, default: &str) -> String {
    s(key).unwrap_or_else(|| default.to_string())
}
fn i_or(key: &str, default: i64) -> i64 {
    s(key).and_then(|v| v.parse().ok()).unwrap_or(default)
}
fn f_or(key: &str, default: f64) -> f64 {
    s(key).and_then(|v| v.parse().ok()).unwrap_or(default)
}
fn truthy(key: &str) -> bool {
    matches!(s(key).as_deref(), Some("1") | Some("true"))
}

#[derive(Clone, Debug)]
pub struct Config {
    pub backend_port: u16,
    pub external_url: String,
    pub database_url: String,
    pub jwt_secret: String,
    pub frontend_host: String,
    pub frontend_port: u16,
    pub frontend_dir: String,
    pub is_production: bool,

    pub cooldown_ms: i64,
    pub active_cooldown_ms: i64,
    pub booster_cooldown_ms: i64,
    pub special_cooldown_ms: i64,
    pub cooldown_override_for_special: bool,

    pub level_base_pixel: f64,
    pub level_exponent: f64,
    pub level_up_droplets_reward: i64,
    pub level_up_max_charges_reward: f64,
    pub painted_droplets_reward: i64,

    pub ban_on_banned_ip: bool,
    pub block_tor: bool,

    pub enable_rate_limit: bool,
    pub login_rate_limit_attempts: i64,
    pub login_rate_limit_ms: i64,
    pub signup_rate_limit_attempts: i64,
    pub signup_rate_limit_ms: i64,
    pub password_reset_rate_limit_attempts: i64,
    pub password_reset_rate_limit_ms: i64,
    pub paint_rate_limit_attempts: i64,
    pub paint_rate_limit_ms: i64,

    pub discord_client_id: Option<String>,
    pub discord_client_secret: Option<String>,
    pub discord_redirect_url: Option<String>,
    pub discord_bot_token: Option<String>,
    pub discord_server_id: Option<String>,
    pub discord_active_role_ids: Vec<String>,
    pub discord_booster_role_ids: Vec<String>,

    pub allow_multi_account: bool,
    pub allow_offensive_content: bool,
    pub allow_explicit_content: bool,
    pub allow_griefing: bool,
    pub allow_kind_griefing: bool,
    pub allow_political_griefing: bool,
    pub allow_vpn: bool,
    pub allow_bots: bool,
    pub extra_rules: Option<String>,

    // Performance tunables (new in the Rust backend)
    pub db_max_connections: u32,
    pub session_cache_ttl_ms: u64,
    pub user_cache_ttl_ms: u64,
    pub tile_cache_max_tiles: usize,
    pub tile_flush_ms: u64,
    pub stats_flush_ms: u64,
    pub body_limit_bytes: usize,

    // Anti-automation (self-hosted fingerprinting + PoW gate)
    pub anti_bot_mode: String,
    pub anti_bot_key: Option<String>,
    pub anti_bot_pow_bits: u8,
    pub anti_bot_enforce_threshold: i32,
}

impl Config {
    pub fn from_env() -> Self {
        let port = s("BACKEND_PORT")
            .or_else(|| s("PORT"))
            .and_then(|v| v.parse().ok())
            .unwrap_or(3000);
        let frontend_port = s("FRONTEND_PORT")
            .and_then(|v| v.parse().ok())
            .unwrap_or(3001);
        let parse_csv = |v: Option<String>| -> Vec<String> {
            v.map(|s| {
                s.split(',')
                    .map(|p| p.trim().to_string())
                    .filter(|p| !p.is_empty())
                    .collect()
            })
            .unwrap_or_default()
        };
        Self {
            backend_port: port,
            external_url: s_or("EXTERNAL_URL", "http://localhost:3000"),
            database_url: s("DATABASE_URL").unwrap_or_else(|| {
                "postgres://postgres:postgres@localhost:5432/openplace".to_string()
            }),
            jwt_secret: s("JWT_SECRET").unwrap_or_else(|| {
                // The JS backend panics without JWT_SECRET; keep that contract but allow a dev default.
                eprintln!("[warn] JWT_SECRET is not set — using an insecure development default");
                "insecure-dev-secret".to_string()
            }),
            frontend_host: s_or("FRONTEND_HOST", "localhost"),
            frontend_port,
            frontend_dir: s_or("FRONTEND_DIR", "./frontend"),
            is_production: s_or("NODE_ENV", "") == "production",

            cooldown_ms: i_or("COOLDOWN_MS", 30_000),
            active_cooldown_ms: i_or("ACTIVE_COOLDOWN_MS", 15_000),
            booster_cooldown_ms: i_or("BOOSTER_COOLDOWN_MS", 10_000),
            special_cooldown_ms: i_or("SPECIAL_COOLDOWN_MS", 5_000),
            cooldown_override_for_special: truthy("COOLDOWN_OVERRIDE_FOR_SPECIAL"),

            level_base_pixel: f_or("LEVEL_BASE_PIXEL", 30.0),
            level_exponent: f_or("LEVEL_EXPONENT", 0.65),
            level_up_droplets_reward: i_or("LEVEL_UP_DROPLETS_REWARD", 500),
            level_up_max_charges_reward: f_or("LEVEL_UP_MAX_CHARGES_REWARD", 2.0),
            painted_droplets_reward: i_or("PAINTED_DROPLETS_REWARD", 1),

            ban_on_banned_ip: truthy("BAN_ON_BANNED_IP"),
            block_tor: truthy("BLOCK_TOR"),

            enable_rate_limit: truthy("ENABLE_RATE_LIMIT"),
            login_rate_limit_attempts: i_or("LOGIN_RATE_LIMIT_ATTEMPTS", 5),
            login_rate_limit_ms: i_or("LOGIN_RATE_LIMIT_MS", 300_000),
            signup_rate_limit_attempts: i_or("SIGNUP_RATE_LIMIT_ATTEMPTS", 2),
            signup_rate_limit_ms: i_or("SIGNUP_RATE_LIMIT_MS", 3_600_000),
            password_reset_rate_limit_attempts: i_or("PASSWORD_RESET_RATE_LIMIT_ATTEMPTS", 3),
            password_reset_rate_limit_ms: i_or("PASSWORD_RESET_RATE_LIMIT_MS", 600_000),
            paint_rate_limit_attempts: i_or("PAINT_RATE_LIMIT_ATTEMPTS", 60),
            paint_rate_limit_ms: i_or("PAINT_RATE_LIMIT_MS", 10_000),

            discord_client_id: s("DISCORD_CLIENT_ID"),
            discord_client_secret: s("DISCORD_CLIENT_SECRET"),
            discord_redirect_url: s("DISCORD_REDIRECT_URL"),
            discord_bot_token: s("DISCORD_BOT_TOKEN"),
            discord_server_id: s("DISCORD_SERVER_ID"),
            discord_active_role_ids: parse_csv(s("DISCORD_ACTIVE_ROLE_IDS")),
            discord_booster_role_ids: parse_csv(s("DISCORD_BOOSTER_ROLE_IDS")),

            allow_multi_account: truthy("ALLOW_MULTI_ACCOUNT"),
            allow_offensive_content: truthy("ALLOW_OFFENSIVE_CONTENT"),
            allow_explicit_content: truthy("ALLOW_EXPLICIT_CONTENT"),
            allow_griefing: truthy("ALLOW_GRIEFING"),
            allow_kind_griefing: s("ALLOW_KIND_GRIEFING")
                .map(|v| matches!(v.as_str(), "1" | "true"))
                .unwrap_or(true),
            allow_political_griefing: truthy("ALLOW_POLITICAL_GRIEFING"),
            allow_vpn: truthy("ALLOW_VPN"),
            allow_bots: truthy("ALLOW_BOTS"),
            extra_rules: s("EXTRA_RULES"),

            db_max_connections: i_or("DB_MAX_CONNECTIONS", 32).max(1) as u32,
            session_cache_ttl_ms: i_or("SESSION_CACHE_TTL_MS", 60_000).max(0) as u64,
            user_cache_ttl_ms: i_or("USER_CACHE_TTL_MS", 30_000).max(0) as u64,
            tile_cache_max_tiles: i_or("TILE_CACHE_MAX_TILES", 512).max(16) as usize,
            tile_flush_ms: i_or("TILE_FLUSH_MS", 500).max(10) as u64,
            stats_flush_ms: i_or("STATS_FLUSH_MS", 1000).max(10) as u64,
            body_limit_bytes: i_or("BODY_LIMIT_BYTES", 50 * 1024 * 1024).max(1) as usize,

            anti_bot_mode: s_or("ANTI_BOT_MODE", "log"),
            anti_bot_key: s("ANTI_BOT_KEY"),
            anti_bot_pow_bits: i_or("ANTI_BOT_POW_BITS", 18).clamp(0, 255) as u8,
            anti_bot_enforce_threshold: i_or("ANTI_BOT_ENFORCE_THRESHOLD", 100) as i32,
        }
    }

    pub fn discord_configured(&self) -> bool {
        self.discord_client_id.is_some()
            && self.discord_client_secret.is_some()
            && self.discord_redirect_url.is_some()
    }

    /// Anti-automation system is entirely disabled (no script, no scoring).
    pub fn is_off(&self) -> bool {
        self.anti_bot_mode == "off"
    }

    /// Anti-automation blocks flagged users instead of only logging.
    pub fn is_enforce(&self) -> bool {
        self.anti_bot_mode == "enforce"
    }

    /// Dedicated anti-bot HMAC key, or one derived from the JWT secret.
    pub fn antibot_key(&self) -> String {
        self.anti_bot_key
            .clone()
            .unwrap_or_else(|| format!("{}:antibot", self.jwt_secret))
    }
}
