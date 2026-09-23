use std::sync::Arc;

use crate::auth::SessionCache;
use crate::config::Config;
use crate::rate_limiter::RateLimiter;
use crate::services::antibot::AntibotStore;
use crate::services::leaderboard::LeaderboardService;
use crate::services::region::RegionService;
use crate::services::settings::RuntimeSettings;
use crate::services::tiles::TileStore;
use crate::services::user_cache::UserCache;
use sqlx::PgPool;
use tokio::sync::mpsc;

#[derive(Clone)]
pub struct AppState(pub Arc<Inner>);

impl std::ops::Deref for AppState {
    type Target = Inner;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

pub struct Inner {
    pub config: Config,
    pub frontend_dir: String,
    pub pool: PgPool,
    pub sessions: SessionCache,
    pub users: UserCache,
    pub tiles: Arc<TileStore>,
    pub regions: RegionService,
    pub limiter: Arc<RateLimiter>,
    pub stats: Arc<crate::services::stats::StatsQueue>,
    pub leaderboard: Arc<LeaderboardService>,
    /// City ids whose region leaderboards must be invalidated (all 4 modes).
    pub region_invalidate_tx: mpsc::UnboundedSender<i32>,
    pub http: reqwest::Client,
    pub discord_links: Arc<crate::services::user_ops::DiscordLinkStates>,
    pub antibot: Arc<AntibotStore>,
    /// Runtime-editable instance settings (community rules, bot defense).
    /// std RwLock: only ever held for a cheap clone, never across await.
    pub settings: Arc<std::sync::RwLock<RuntimeSettings>>,
}

impl Inner {
    /// Cheap read-lock snapshot of the runtime settings.
    pub fn settings_snapshot(&self) -> RuntimeSettings {
        self.settings.read().unwrap().clone()
    }
}
