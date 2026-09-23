use std::sync::Arc;

use clap::{Parser, Subcommand};
use sqlx::postgres::PgPoolOptions;

use openplace_backend::config::Config;
use openplace_backend::rate_limiter::RateLimiter;
use openplace_backend::routes;
use openplace_backend::services::antibot::AntibotStore;
use openplace_backend::services::leaderboard::LeaderboardService;
use openplace_backend::services::region::RegionService;
use openplace_backend::services::settings;
use openplace_backend::services::stats::StatsQueue;
use openplace_backend::services::tiles::TileStore;
use openplace_backend::services::user_cache::UserCache;
use openplace_backend::services::user_ops::DiscordLinkStates;
use openplace_backend::state::{AppState, Inner};
use openplace_backend::{auth, discord};

#[derive(Parser)]
#[command(name = "openplace-backend", version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run the HTTP server (default workload).
    Serve,
    /// Health probe for container orchestration: GET /health on BACKEND_PORT.
    Healthcheck,
    /// Run migrations + seed system users (-1 System, -2 Deleted Account).
    Setup,
    /// Import a GeoNames dump (cities500/cities1000/... zip or TSV).
    ImportGeonames { path: String },
    /// Re-render every tile blob from pixel rows.
    RedrawTiles,
    /// Initialize leaderboard views.
    InitLeaderboard,
    /// Import a banned-IP list file (one IP/CIDR per line, # comments).
    ImportIpList {
        path: String,
        /// Suspension reason for the imported ranges.
        #[arg(long, default_value = "ip-list")]
        reason: String,
    },
    /// Create a system-wide notification (shown in every user's inbox).
    SystemNotification {
        title: String,
        message: String,
        #[arg(long, default_value = "system")]
        icon: String,
    },
    /// Seed synthetic benchmark data (regions/users/pixels/tiles).
    SeedBench {
        #[arg(long, default_value = "1000")]
        regions: i64,
        #[arg(long, default_value = "5000")]
        users: i64,
        #[arg(long, default_value = "40")]
        tiles: i64,
    },
    /// Migrate data from the legacy Node.js backend (MariaDB/MySQL).
    MigrateFromMysql {
        /// mysql:// URL of the source database.
        source: String,
        /// Allow migrating into a non-empty target database.
        #[arg(long)]
        force: bool,
    },
}

#[tokio::main]
async fn main() -> anyhow_result::Result<()> {
    let cli = Cli::parse();
    let config = Config::from_env();

    match cli.command {
        Command::Serve => serve(config).await,
        Command::Healthcheck => healthcheck(&config).await,
        Command::Setup => openplace_backend::setup::run_setup(&config).await,
        Command::ImportGeonames { path } => {
            openplace_backend::setup::import_geonames(&config, &path).await
        }
        Command::ImportIpList { path, reason } => {
            openplace_backend::setup::import_ip_list(&config, &path, &reason).await
        }
        Command::SystemNotification {
            title,
            message,
            icon,
        } => openplace_backend::setup::system_notification(&config, &title, &message, &icon).await,
        Command::RedrawTiles => openplace_backend::setup::redraw_tiles(&config).await,
        Command::InitLeaderboard => openplace_backend::setup::init_leaderboard(&config).await,
        Command::SeedBench {
            regions,
            users,
            tiles,
        } => openplace_backend::setup::seed_bench(&config, regions, users, tiles).await,
        Command::MigrateFromMysql { source, force } => {
            openplace_backend::migrate::run(&config, &source, force).await
        }
    }
}

async fn serve(config: Config) -> anyhow_result::Result<()> {
    let pool = PgPoolOptions::new()
        .max_connections(config.db_max_connections)
        .acquire_timeout(std::time::Duration::from_secs(10))
        .connect(&config.database_url)
        .await?;

    match sqlx::migrate!("./migrations").run(&pool).await {
        Ok(_) => {}
        Err(sqlx::migrate::MigrateError::VersionMismatch(_)) => {
            eprintln!("[migrate] version mismatch — skipping (schema already applied)");
        }
        Err(e) => return Err(e.into()),
    }

    let sessions = auth::SessionCache::new(config.session_cache_ttl_ms);
    let users = UserCache::new(config.user_cache_ttl_ms);
    let tiles = TileStore::new(config.tile_cache_max_tiles);
    let regions = RegionService::new(pool.clone());
    let limiter = Arc::new(RateLimiter::new());
    limiter.clone().start_cleanup();

    let stats = StatsQueue::new();
    let leaderboard = Arc::new(LeaderboardService::new(pool.clone()));
    let (region_invalidate_tx, region_invalidate_rx) = tokio::sync::mpsc::unbounded_channel();

    let http = reqwest::Client::builder().build().expect("reqwest client");
    let antibot = Arc::new(AntibotStore::new());
    antibot.clone().start_cleanup();

    // Runtime settings: DB values layered over the env defaults (pool →
    // settings → state, so AppState is built with the loaded snapshot).
    let runtime_settings = settings::load(&pool, &config).await;
    if runtime_settings.anti_bot_mode != "off" {
        eprintln!(
            "[antibot] mode={} pow_bits={}",
            runtime_settings.anti_bot_mode, config.anti_bot_pow_bits
        );
    }

    let state = AppState(Arc::new(Inner {
        config: config.clone(),
        frontend_dir: config.frontend_dir.clone(),
        pool: pool.clone(),
        sessions,
        users,
        tiles: tiles.clone(),
        regions,
        limiter,
        stats: stats.clone(),
        leaderboard: leaderboard.clone(),
        region_invalidate_tx,
        http,
        discord_links: Arc::new(DiscordLinkStates::new()),
        antibot,
        settings: Arc::new(std::sync::RwLock::new(runtime_settings)),
    }));

    // Warm the region tree + spawn background workers.
    state.regions.ensure_loaded().await;
    println!("[openplace-rs] regions loaded: {}", state.regions.count());

    {
        // Self-healing: rebuild tiles whose pixel rows are newer than the
        // stored blob (e.g. after a crash between pixel write and blob flush).
        let pool = pool.clone();
        let tiles = tiles.clone();
        tokio::spawn(async move {
            loop {
                TileStore::reconcile(tiles.clone(), pool.clone()).await;
                tokio::time::sleep(std::time::Duration::from_secs(300)).await;
            }
        });
    }
    {
        let tiles = tiles.clone();
        let pool = pool.clone();
        let flush_ms = config.tile_flush_ms;
        tokio::spawn(async move {
            TileStore::run_flusher(tiles, pool, flush_ms).await;
        });
    }
    {
        let stats = stats.clone();
        let pool = pool.clone();
        let flush_ms = config.stats_flush_ms;
        let tx = state.region_invalidate_tx.clone();
        tokio::spawn(async move {
            StatsQueue::run_flusher(stats, pool, flush_ms, tx).await;
        });
    }
    {
        let lb = leaderboard.clone();
        tokio::spawn(async move {
            loop {
                lb.warmup().await;
                tokio::time::sleep(std::time::Duration::from_secs(60)).await;
            }
        });
    }
    {
        let lb = leaderboard.clone();
        let rx = region_invalidate_rx;
        tokio::spawn(async move {
            lb.run_workers(rx).await;
        });
    }
    discord::bot::start(state.clone()).await;
    state.discord_links.clone().start_cleanup();

    let addr = format!("0.0.0.0:{}", config.backend_port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    println!("[openplace-rs] listening on http://{addr}");

    let app = routes::build_router(state);
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .await?;
    Ok(())
}

async fn healthcheck(config: &Config) -> anyhow_result::Result<()> {
    let url = format!("http://127.0.0.1:{}/health", config.backend_port);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(3))
        .build()?;
    let status = client.get(url).send().await?.status();
    if status.is_success() {
        Ok(())
    } else {
        Err(format!("health endpoint returned {status}").into())
    }
}

mod anyhow_result {
    pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
}
