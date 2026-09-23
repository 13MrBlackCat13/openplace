//! Materialized + realtime leaderboard service.
//!
//! Port of src/services/leaderboard.ts:
//!  * in-memory JSON cache keyed by `{type}:{mode}:{entityId|all}` with a
//!    60 s TTL (JS: memoryCache + 30 s cleanup sweep),
//!  * realtime regionPlayers / regionAlliances queries over
//!    user_region_stats (all-time) / user_region_stats_daily (periods),
//!  * leaderboard_view diff-upserts for player/alliance/country/region,
//!  * a background worker set: region-invalidation consumer (fed by the
//!    stats flusher) + a 3 s throttled view-update queue processor.
//!
//! CONTRACT (do not change signatures — routes depend on them):
//!  * `get` returns the JSON array for GET /leaderboard/... and
//!    /alliance/leaderboard/... (top-N entries; the JS backend uses 50).
//!  * `invalidate` mirrors leaderboardService.invalidateLeaderboard.
//!  * `run_workers` consumes the shared region-invalidation channel (fed by
//!    the paint pipeline / stats flusher) and the internal update queue.
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use dashmap::DashMap;
use serde_json::{json, Map, Value};
use sqlx::{FromRow, PgPool};
use tokio::sync::mpsc;
use tokio::time::MissedTickBehavior;

use crate::error::ApiResult;
use crate::services::stats::period_start;

const CACHE_TTL_MS: i64 = 60_000;
const CLEANUP_INTERVAL_MS: u64 = 30_000;
const UPDATE_INTERVAL_MS: u64 = 3_000;
const BATCH_SIZE: usize = 5;
const MAX_QUEUE_SIZE: usize = 1000;
const ALL_MODES: [&str; 4] = ["today", "week", "month", "all-time"];
const WARM_MODES_SHORT: [&str; 3] = ["today", "week", "month"]; // player / alliance
const ALL_TIME: &str = "all-time";

pub struct LeaderboardService {
    pub pool: PgPool,
    /// `{type}:{mode}:{entityId|all}` → (entries, cached-at ms).
    cache: DashMap<(String, String, Option<i32>), (Value, i64)>,
    /// Pending view-update keys (same strings the JS updateQueue holds).
    update_queue: DashMap<String, ()>,
}

// ---------------------------------------------------------------------------
// SQL row types
// ---------------------------------------------------------------------------

#[derive(FromRow)]
struct AllTimePlayerRow {
    id: i32,
    name: String,
    picture: Option<String>,
    alliance_id: Option<i32>,
    alliance_name: Option<String>,
    equipped_flag: i32,
    discord: Option<String>,
    pixels_painted: i32,
}

/// One leaderboard_view row (rank only drives the ORDER BY).
#[derive(FromRow)]
struct ViewRow {
    entity_id: Option<i32>,
    pixels_painted: i32,
}

#[derive(FromRow)]
struct EnrichUserRow {
    id: i32,
    name: String,
    picture: Option<String>,
    alliance_id: Option<i32>,
    alliance_name: Option<String>,
    equipped_flag: i32,
    discord: Option<String>,
}

#[derive(FromRow)]
struct EnrichAllianceRow {
    id: i32,
    name: String,
    pixels_painted: i32,
}

#[derive(FromRow)]
struct EnrichRegionRow {
    id: i32,
    city_id: i32,
    name: String,
    number: i32,
    country_id: i32,
    latitude: f64,
    longitude: f64,
}

#[derive(FromRow)]
struct RegionPlayerRow {
    user_id: i32,
    name: String,
    picture: Option<String>,
    equipped_flag: i32,
    discord: Option<String>,
    alliance_id: Option<i32>,
    alliance_name: Option<String>,
    painted: i64,
}

#[derive(FromRow)]
struct RegionAllianceRow {
    alliance_id: i32,
    name: Option<String>,
    painted: i64,
}

/// Generic "group by id, sum pixels" row (user/country/region groupings).
#[derive(FromRow)]
struct GroupCountRow {
    id: i32,
    painted: i64,
}

#[derive(FromRow)]
struct AllianceTopRow {
    id: i32,
    pixels_painted: i32,
}

#[derive(FromRow)]
struct AlliancePeriodRow {
    id: i32,
    total: i64,
}

impl LeaderboardService {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            cache: DashMap::new(),
            update_queue: DashMap::new(),
        }
    }

    /// Returns the leaderboard rows as a JSON array.
    pub async fn get(
        &self,
        lb_type: &str,
        mode: &str,
        entity_id: Option<i32>,
        limit: i64,
    ) -> ApiResult<Value> {
        // Real-time data first for region-specific leaderboards (JS: always).
        if lb_type == "regionPlayers" || lb_type == "regionAlliances" {
            if let Some(city) = entity_id.filter(|c| *c != 0) {
                let realtime = if lb_type == "regionPlayers" {
                    self.region_players_realtime(city, limit, mode).await
                } else {
                    self.region_alliances_realtime(city, limit, mode).await
                };
                match realtime {
                    Ok(entries) if !entries.is_empty() => {
                        return Ok(Value::Array(entries));
                    }
                    Ok(_) => {}
                    Err(err) => {
                        // JS swallows realtime errors and falls through to the
                        // (empty) view / memory cache.
                        eprintln!("[leaderboard] realtime {lb_type}:{mode}:{city} error: {err}");
                    }
                }
            }
        }

        let cache_key = cache_key(lb_type, mode, entity_id);
        let mut entries = self.get_from_view(lb_type, mode, entity_id, limit).await?;
        if entries.is_empty() {
            if let Some(cached) = self.cache.get(&cache_key) {
                let (items, ts) = cached.value();
                if now_ms() - *ts <= CACHE_TTL_MS
                    && !items.as_array().map(|a| a.is_empty()).unwrap_or(false)
                {
                    entries = slice_items(items, limit);
                }
            }
        }

        // JS: entries empty OR the key is already pending → (re)queue.
        let key = key_string(lb_type, mode, entity_id);
        if entries.is_empty() || self.update_queue.contains_key(&key) {
            self.queue_update(&key);
        }

        Ok(Value::Array(entries))
    }

    /// Mirrors leaderboardService.invalidateLeaderboard: region-scoped caches
    /// are dropped immediately, global keys go into the update queue.
    pub async fn invalidate(&self, lb_type: &str, mode: Option<&str>, entity_id: Option<i32>) {
        let mode = mode.unwrap_or(ALL_TIME);
        let key = key_string(lb_type, mode, entity_id);

        if self.update_queue.len() >= MAX_QUEUE_SIZE {
            // JS drops the oldest 100; a DashMap has no insertion order, so an
            // arbitrary 100 are dropped instead.
            let dropped: Vec<String> = self
                .update_queue
                .iter()
                .take(100)
                .map(|e| e.key().clone())
                .collect();
            for k in &dropped {
                self.update_queue.remove(k);
            }
            eprintln!(
                "[leaderboard] queue overflow (>{MAX_QUEUE_SIZE}), dropped {} entries",
                dropped.len()
            );
        }
        self.update_queue.insert(key, ());

        if lb_type == "regionPlayers"
            || lb_type == "regionAlliances"
            || (lb_type == "region" && entity_id.filter(|e| *e != 0).is_some())
        {
            self.cache.remove(&cache_key(lb_type, mode, entity_id));
        }
    }

    /// Warm the materialized views (JS initializeAllLeaderboards):
    /// player/alliance for today/week/month, country/region for all modes.
    /// Must not fail on an empty database.
    pub async fn warmup(&self) {
        for lb_type in ["player", "alliance"] {
            for mode in WARM_MODES_SHORT {
                if let Err(err) = self.update_leaderboard_view(lb_type, mode, None).await {
                    eprintln!("[leaderboard] warmup {lb_type}:{mode} error: {err}");
                }
                self.refresh_cache(lb_type, mode, None).await;
            }
        }
        for lb_type in ["country", "region"] {
            for mode in ALL_MODES {
                if let Err(err) = self.update_leaderboard_view(lb_type, mode, None).await {
                    eprintln!("[leaderboard] warmup {lb_type}:{mode} error: {err}");
                }
                self.refresh_cache(lb_type, mode, None).await;
            }
        }
    }

    /// Background workers: consume the region-invalidation channel (a city id)
    /// and drain the view-update queue on a 3 s token timer. Also runs the
    /// 30 s cache-TTL sweep (JS startCacheCleanup).
    pub async fn run_workers(self: Arc<Self>, mut region_rx: mpsc::UnboundedReceiver<i32>) {
        let mut tick = tokio::time::interval(Duration::from_millis(UPDATE_INTERVAL_MS));
        tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
        let mut cleanup = tokio::time::interval(Duration::from_millis(CLEANUP_INTERVAL_MS));
        cleanup.set_missed_tick_behavior(MissedTickBehavior::Delay);

        loop {
            tokio::select! {
                city = region_rx.recv() => {
                    let Some(city) = city else { break };
                    for mode in ALL_MODES {
                        self.invalidate("regionPlayers", Some(mode), Some(city)).await;
                        self.invalidate("regionAlliances", Some(mode), Some(city)).await;
                    }
                }
                _ = tick.tick() => {
                    self.process_queue_batch().await;
                }
                _ = cleanup.tick() => {
                    self.cleanup_cache();
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // View reads + enrichment
    // -----------------------------------------------------------------------

    /// JS getFromView: all-time player/alliance read their base tables, the
    /// rest read leaderboard_view; empty region views fall back to a
    /// per-country stats grouping.
    async fn get_from_view(
        &self,
        lb_type: &str,
        mode: &str,
        entity_id: Option<i32>,
        limit: i64,
    ) -> ApiResult<Vec<Value>> {
        // regionPlayers/regionAlliances are realtime-only — never in the view.
        if lb_type == "regionPlayers" || lb_type == "regionAlliances" {
            return Ok(Vec::new());
        }

        if mode == ALL_TIME {
            if lb_type == "player" {
                let rows: Vec<AllTimePlayerRow> = sqlx::query_as(
                    r#"
                    SELECT u.id, COALESCE(u.nickname, u.name) AS name, u.picture,
                           u.alliance_id, a.name AS alliance_name,
                           u.equipped_flag, u.discord, u.pixels_painted
                    FROM users u
                    LEFT JOIN alliances a ON a.id = u.alliance_id
                    WHERE u.role = 'user' AND u.pixels_painted > 0
                    ORDER BY u.pixels_painted DESC
                    LIMIT $1
                    "#,
                )
                .bind(limit)
                .fetch_all(&self.pool)
                .await?;
                return Ok(rows
                    .into_iter()
                    .map(|r| {
                        build_player_object(
                            r.id,
                            &r.name,
                            r.picture.as_deref(),
                            r.alliance_id,
                            r.alliance_name.as_deref(),
                            r.equipped_flag,
                            r.discord.as_deref(),
                            r.pixels_painted as i64,
                        )
                    })
                    .collect());
            }
            if lb_type == "alliance" {
                let rows: Vec<EnrichAllianceRow> = sqlx::query_as(
                    "SELECT id, name, pixels_painted FROM alliances \
                     WHERE pixels_painted > 0 ORDER BY pixels_painted DESC LIMIT $1",
                )
                .bind(limit)
                .fetch_all(&self.pool)
                .await?;
                return Ok(rows
                    .into_iter()
                    .map(|a| {
                        json!({
                            "id": a.id,
                            "name": a.name,
                            "pixelsPainted": a.pixels_painted,
                        })
                    })
                    .collect());
            }
        }

        let entity = entity_id.filter(|e| *e != 0);
        let rows: Vec<ViewRow> = sqlx::query_as(
            "SELECT entity_id, pixels_painted FROM leaderboard_view \
             WHERE type = $1 AND mode = $2 AND ($3::int IS NULL OR entity_id = $3) \
             ORDER BY rank ASC LIMIT $4",
        )
        .bind(lb_type)
        .bind(mode)
        .bind(entity)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        if rows.is_empty() {
            if lb_type == "region" {
                if let Some(country_id) = entity {
                    return self.get_region_by_country(mode, country_id, limit).await;
                }
            }
            return Ok(Vec::new());
        }

        self.enrich_entries(lb_type, mode, rows).await
    }

    async fn enrich_entries(
        &self,
        lb_type: &str,
        mode: &str,
        rows: Vec<ViewRow>,
    ) -> ApiResult<Vec<Value>> {
        match lb_type {
            "player" | "regionPlayers" => self.enrich_players(rows).await,
            "alliance" | "regionAlliances" => self.enrich_alliances(mode, rows).await,
            "country" => Ok(rows
                .into_iter()
                .map(|r| {
                    json!({
                        "id": r.entity_id.unwrap_or(0),
                        "pixelsPainted": r.pixels_painted,
                    })
                })
                .collect()),
            "region" => self.enrich_regions(rows).await,
            _ => Ok(Vec::new()),
        }
    }

    async fn enrich_players(&self, rows: Vec<ViewRow>) -> ApiResult<Vec<Value>> {
        let ids: Vec<i32> = rows.iter().filter_map(|r| r.entity_id).collect();
        let mut users: HashMap<i32, EnrichUserRow> = HashMap::new();
        if !ids.is_empty() {
            let fetched: Vec<EnrichUserRow> = sqlx::query_as(
                r#"
                SELECT u.id, COALESCE(u.nickname, u.name) AS name, u.picture,
                       u.alliance_id, a.name AS alliance_name,
                       u.equipped_flag, u.discord
                FROM users u
                LEFT JOIN alliances a ON a.id = u.alliance_id
                WHERE u.role = 'user' AND u.id = ANY($1)
                "#,
            )
            .bind(&ids)
            .fetch_all(&self.pool)
            .await?;
            for u in fetched {
                users.insert(u.id, u);
            }
        }

        Ok(rows
            .into_iter()
            .filter_map(|r| {
                let entity = r.entity_id?;
                let user = users.get(&entity)?;
                Some(build_player_object(
                    user.id,
                    &user.name,
                    user.picture.as_deref(),
                    user.alliance_id,
                    user.alliance_name.as_deref(),
                    user.equipped_flag,
                    user.discord.as_deref(),
                    r.pixels_painted as i64,
                ))
            })
            .collect())
    }

    async fn enrich_alliances(&self, mode: &str, rows: Vec<ViewRow>) -> ApiResult<Vec<Value>> {
        let ids: Vec<i32> = rows.iter().filter_map(|r| r.entity_id).collect();
        let mut alliances: HashMap<i32, EnrichAllianceRow> = HashMap::new();
        if !ids.is_empty() {
            let fetched: Vec<EnrichAllianceRow> =
                sqlx::query_as("SELECT id, name, pixels_painted FROM alliances WHERE id = ANY($1)")
                    .bind(&ids)
                    .fetch_all(&self.pool)
                    .await?;
            for a in fetched {
                alliances.insert(a.id, a);
            }
        }

        Ok(rows
            .into_iter()
            .filter_map(|r| {
                let entity = r.entity_id?;
                let alliance = alliances.get(&entity)?;
                // JS: all-time shows the alliance's own counter, periods show
                // the snapshot value from the view.
                let pixels = if mode == ALL_TIME {
                    alliance.pixels_painted as i64
                } else {
                    r.pixels_painted as i64
                };
                Some(json!({
                    "id": alliance.id,
                    "name": alliance.name,
                    "pixelsPainted": pixels,
                }))
            })
            .collect())
    }

    async fn enrich_regions(&self, rows: Vec<ViewRow>) -> ApiResult<Vec<Value>> {
        let ids: Vec<i32> = rows.iter().filter_map(|r| r.entity_id).collect();
        let mut regions: HashMap<i32, EnrichRegionRow> = HashMap::new();
        if !ids.is_empty() {
            let fetched: Vec<EnrichRegionRow> = sqlx::query_as(
                "SELECT id, city_id, name, number, country_id, latitude, longitude \
                 FROM regions WHERE city_id = ANY($1)",
            )
            .bind(&ids)
            .fetch_all(&self.pool)
            .await?;
            for r in fetched {
                regions.insert(r.city_id, r);
            }
        }

        Ok(rows
            .into_iter()
            .filter_map(|r| {
                let entity = r.entity_id?;
                let region = regions.get(&entity)?;
                Some(json!({
                    "id": region.id,
                    "name": region.name,
                    "pixelsPainted": r.pixels_painted,
                    "cityId": region.city_id,
                    "number": region.number,
                    "countryId": region.country_id,
                    "lastLatitude": region.latitude,
                    "lastLongitude": region.longitude,
                }))
            })
            .collect())
    }

    /// JS getRegionLeaderboardByCountry — stats fallback for
    /// `region:{mode}:{country}` views (the worker never writes those).
    async fn get_region_by_country(
        &self,
        mode: &str,
        country_id: i32,
        limit: i64,
    ) -> ApiResult<Vec<Value>> {
        let cache_key = cache_key("region", mode, Some(country_id));
        if let Some(cached) = self.cache.get(&cache_key) {
            let (items, ts) = cached.value();
            if now_ms() - *ts <= CACHE_TTL_MS
                && !items.as_array().map(|a| a.is_empty()).unwrap_or(false)
            {
                return Ok(slice_items(items, limit));
            }
        }

        let start = period_start(mode);
        let stats: Vec<GroupCountRow> = sqlx::query_as(
            r#"
            SELECT s.region_city_id AS id, SUM(s.pixels_painted) AS painted
            FROM user_region_stats s
            WHERE s.region_city_id IS NOT NULL
              AND s.region_country_id = $1
              AND ($2::timestamptz IS NULL OR s.last_painted_at >= $2)
            GROUP BY s.region_city_id
            ORDER BY painted DESC
            LIMIT $3
            "#,
        )
        .bind(country_id)
        .bind(start)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        if stats.is_empty() {
            return Ok(Vec::new());
        }

        let city_ids: Vec<i32> = stats.iter().map(|s| s.id).collect();
        let regions = self.load_regions(&city_ids).await?;

        let mut entries: Vec<Value> = Vec::new();
        for stat in stats {
            let Some(region) = regions.get(&stat.id) else {
                continue;
            };
            entries.push(build_region_object(region, stat.painted));
            if entries.len() as i64 >= limit {
                break;
            }
        }

        if !entries.is_empty() {
            self.cache
                .insert(cache_key, (Value::Array(entries.clone()), now_ms()));
        }
        Ok(entries)
    }

    async fn load_regions(&self, city_ids: &[i32]) -> ApiResult<HashMap<i32, EnrichRegionRow>> {
        let mut map = HashMap::new();
        if !city_ids.is_empty() {
            let rows: Vec<EnrichRegionRow> = sqlx::query_as(
                "SELECT id, city_id, name, number, country_id, latitude, longitude \
                 FROM regions WHERE city_id = ANY($1)",
            )
            .bind(city_ids)
            .fetch_all(&self.pool)
            .await?;
            for r in rows {
                map.insert(r.city_id, r);
            }
        }
        Ok(map)
    }

    // -----------------------------------------------------------------------
    // Realtime region leaderboards
    // -----------------------------------------------------------------------

    async fn region_players_realtime(
        &self,
        city_id: i32,
        limit: i64,
        mode: &str,
    ) -> ApiResult<Vec<Value>> {
        // all-time over the lifetime table; periods over the daily rollup
        // (unknown modes behave like JS's default empty date filter).
        let (table, date_col) = if mode == ALL_TIME {
            ("user_region_stats", "last_painted_at")
        } else {
            ("user_region_stats_daily", "date")
        };
        let start = if mode == ALL_TIME {
            None
        } else {
            period_start(mode)
        };
        let sql = format!(
            r#"
            SELECT s.user_id,
                   COALESCE(u.nickname, u.name) AS name,
                   u.picture, u.equipped_flag, u.discord,
                   u.alliance_id, a.name AS alliance_name,
                   SUM(s.pixels_painted) AS painted
            FROM {table} s
            JOIN users u ON u.id = s.user_id
            LEFT JOIN alliances a ON a.id = u.alliance_id
            WHERE s.region_city_id = $1
              AND u.role = 'user'
              AND ($2::timestamptz IS NULL OR s.{date_col} >= $2)
            GROUP BY s.user_id, u.nickname, u.name, u.picture,
                     u.equipped_flag, u.discord, u.alliance_id, a.name
            HAVING SUM(s.pixels_painted) > 0
            ORDER BY painted DESC
            LIMIT $3
            "#
        );
        let rows: Vec<RegionPlayerRow> = sqlx::query_as(&sql)
            .bind(city_id)
            .bind(start)
            .bind(limit)
            .fetch_all(&self.pool)
            .await?;

        Ok(rows
            .into_iter()
            .map(|r| {
                let mut o = Map::new();
                o.insert("id".into(), json!(r.user_id));
                o.insert("name".into(), json!(r.name));
                o.insert("pixelsPainted".into(), json!(r.painted));
                o.insert("allianceId".into(), json!(r.alliance_id.unwrap_or(0)));
                o.insert(
                    "allianceName".into(),
                    json!(r.alliance_name.unwrap_or_default()),
                );
                if let Some(p) = r.picture.as_deref().filter(|p| !p.is_empty()) {
                    o.insert("picture".into(), json!(p));
                }
                if r.equipped_flag != 0 {
                    o.insert("equippedFlag".into(), json!(r.equipped_flag));
                }
                if let Some(d) = r.discord.as_deref().filter(|d| !d.is_empty()) {
                    o.insert("discord".into(), json!(d));
                }
                Value::Object(o)
            })
            .collect())
    }

    async fn region_alliances_realtime(
        &self,
        city_id: i32,
        limit: i64,
        mode: &str,
    ) -> ApiResult<Vec<Value>> {
        let (table, date_col) = if mode == ALL_TIME {
            ("user_region_stats", "last_painted_at")
        } else {
            ("user_region_stats_daily", "date")
        };
        let start = if mode == ALL_TIME {
            None
        } else {
            period_start(mode)
        };
        // JS has no >0 filter here — zero-sum alliances stay listed (name
        // falls back to "" for alliances whose row disappeared).
        let sql = format!(
            r#"
            SELECT s.alliance_id, a.name AS name, SUM(s.pixels_painted) AS painted
            FROM {table} s
            LEFT JOIN alliances a ON a.id = s.alliance_id
            WHERE s.region_city_id = $1
              AND s.alliance_id IS NOT NULL
              AND ($2::timestamptz IS NULL OR s.{date_col} >= $2)
            GROUP BY s.alliance_id, a.name
            ORDER BY painted DESC
            LIMIT $3
            "#
        );
        let rows: Vec<RegionAllianceRow> = sqlx::query_as(&sql)
            .bind(city_id)
            .bind(start)
            .bind(limit)
            .fetch_all(&self.pool)
            .await?;

        Ok(rows
            .into_iter()
            .map(|r| {
                let name = r.name.unwrap_or_default();
                json!({
                    "id": r.alliance_id,
                    "name": name,
                    "pixelsPainted": r.painted,
                    "allianceId": r.alliance_id,
                    "allianceName": name,
                    "picture": "",
                })
            })
            .collect())
    }

    // -----------------------------------------------------------------------
    // View updates (background)
    // -----------------------------------------------------------------------

    async fn update_leaderboard_view(
        &self,
        lb_type: &str,
        mode: &str,
        _entity_id: Option<i32>,
    ) -> ApiResult<()> {
        let start = period_start(mode);
        match lb_type {
            "player" => {
                // all-time reads the users table directly — nothing to store.
                if mode == ALL_TIME {
                    return Ok(());
                }
                let rows: Vec<GroupCountRow> = sqlx::query_as(
                    r#"
                    SELECT user_id AS id, SUM(pixels_painted) AS painted
                    FROM user_region_stats
                    WHERE ($1::timestamptz IS NULL OR last_painted_at >= $1)
                    GROUP BY user_id
                    ORDER BY painted DESC
                    LIMIT 50
                    "#,
                )
                .bind(start)
                .fetch_all(&self.pool)
                .await?;
                let entries = to_ranked_entries(rows.into_iter().map(|r| (r.id, r.painted)));
                self.update_leaderboard_entries("player", mode, &entries)
                    .await
            }
            "alliance" => {
                let entries = if mode == ALL_TIME {
                    let rows: Vec<AllianceTopRow> = sqlx::query_as(
                        "SELECT id, pixels_painted FROM alliances \
                         WHERE pixels_painted > 0 ORDER BY pixels_painted DESC LIMIT 50",
                    )
                    .fetch_all(&self.pool)
                    .await?;
                    rows.into_iter()
                        .filter(|r| r.pixels_painted > 0)
                        .enumerate()
                        .map(|(i, r)| (r.id, i as i32 + 1, r.pixels_painted))
                        .collect()
                } else {
                    // Top-50 candidate alliances, then a single aggregate over
                    // pixels painted by members after GREATEST(join, period
                    // start) — replaces the JS N+1 member/pixel-count loop.
                    let rows: Vec<AlliancePeriodRow> = sqlx::query_as(
                        r#"
                        WITH top AS (
                            SELECT id FROM alliances
                            WHERE pixels_painted > 0
                            ORDER BY pixels_painted DESC
                            LIMIT 50
                        )
                        SELECT mu.alliance_id AS id, COUNT(*) AS total
                        FROM pixels p
                        JOIN users mu ON mu.id = p.painted_by
                        WHERE mu.alliance_id IN (SELECT id FROM top)
                          AND mu.alliance_joined_at IS NOT NULL
                          AND p.painted_at >= GREATEST(mu.alliance_joined_at, $1::timestamptz)
                        GROUP BY mu.alliance_id
                        ORDER BY total DESC
                        LIMIT 50
                        "#,
                    )
                    .bind(start)
                    .fetch_all(&self.pool)
                    .await?;
                    to_ranked_entries(rows.into_iter().map(|r| (r.id, r.total)))
                };
                self.update_leaderboard_entries("alliance", mode, &entries)
                    .await
            }
            "country" => {
                let rows: Vec<GroupCountRow> = sqlx::query_as(
                    r#"
                    SELECT region_country_id AS id, SUM(pixels_painted) AS painted
                    FROM user_region_stats
                    WHERE region_country_id IS NOT NULL
                      AND ($1::timestamptz IS NULL OR last_painted_at >= $1)
                    GROUP BY region_country_id
                    ORDER BY painted DESC
                    LIMIT 50
                    "#,
                )
                .bind(start)
                .fetch_all(&self.pool)
                .await?;
                let entries = to_ranked_entries(rows.into_iter().map(|r| (r.id, r.painted)));
                self.update_leaderboard_entries("country", mode, &entries)
                    .await
            }
            "region" => {
                let rows: Vec<GroupCountRow> = sqlx::query_as(
                    r#"
                    SELECT region_city_id AS id, SUM(pixels_painted) AS painted
                    FROM user_region_stats
                    WHERE region_city_id IS NOT NULL
                      AND ($1::timestamptz IS NULL OR last_painted_at >= $1)
                    GROUP BY region_city_id
                    ORDER BY painted DESC
                    LIMIT 50
                    "#,
                )
                .bind(start)
                .fetch_all(&self.pool)
                .await?;
                let entries = to_ranked_entries(rows.into_iter().map(|r| (r.id, r.painted)));
                self.update_leaderboard_entries("region", mode, &entries)
                    .await
            }
            // Realtime types never touch the view (JS parity).
            _ => Ok(()),
        }
    }

    /// Diff-upsert: drop view rows no longer in the fresh top-N, upsert the
    /// rest on (type, mode, entity_id, region_id).
    async fn update_leaderboard_entries(
        &self,
        lb_type: &str,
        mode: &str,
        entries: &[(i32, i32, i32)], // (entity_id, rank, pixels_painted)
    ) -> ApiResult<()> {
        let ids: Vec<i32> = entries.iter().map(|e| e.0).collect();
        let mut tx = self.pool.begin().await?;

        sqlx::query(
            "DELETE FROM leaderboard_view \
             WHERE type = $1 AND mode = $2 AND entity_id IS NOT NULL AND entity_id <> ALL($3)",
        )
        .bind(lb_type)
        .bind(mode)
        .bind(&ids)
        .execute(&mut *tx)
        .await?;

        if !entries.is_empty() {
            let mut values = String::new();
            for (i, _) in entries.iter().enumerate() {
                if i > 0 {
                    values.push_str(", ");
                }
                let base = 3 + i * 3;
                values.push_str(&format!(
                    "($1, $2, ${base}, NULL, ${}, ${}, now())",
                    base + 1,
                    base + 2
                ));
            }
            let sql = format!(
                "INSERT INTO leaderboard_view \
                 (type, mode, entity_id, region_id, rank, pixels_painted, last_updated) \
                 VALUES {values} \
                 ON CONFLICT (type, mode, entity_id, region_id) DO UPDATE \
                 SET rank = EXCLUDED.rank, \
                     pixels_painted = EXCLUDED.pixels_painted, \
                     last_updated = now()"
            );
            let mut q = sqlx::query(&sql).bind(lb_type).bind(mode);
            for &(entity_id, rank, pixels) in entries {
                q = q.bind(entity_id).bind(rank).bind(pixels);
            }
            q.execute(&mut *tx).await?;
        }

        tx.commit().await?;
        Ok(())
    }

    async fn refresh_cache(&self, lb_type: &str, mode: &str, entity_id: Option<i32>) {
        match self.get_from_view(lb_type, mode, entity_id, 50).await {
            Ok(items) if !items.is_empty() => {
                self.cache.insert(
                    cache_key(lb_type, mode, entity_id),
                    (Value::Array(items), now_ms()),
                );
            }
            Ok(_) => {}
            Err(err) => {
                eprintln!("[leaderboard] refresh {lb_type}:{mode} error: {err}");
            }
        }
    }

    /// JS processUpdateQueue: take up to BATCH_SIZE keys, refresh their views
    /// + caches, keep going every 500 ms while the queue is non-empty.
    async fn process_queue_batch(&self) {
        while !self.update_queue.is_empty() {
            let keys: Vec<String> = self
                .update_queue
                .iter()
                .take(BATCH_SIZE)
                .map(|e| e.key().clone())
                .collect();
            if keys.is_empty() {
                break;
            }
            for key in &keys {
                self.update_queue.remove(key);
            }

            for key in keys {
                let Some((lb_type, mode, entity_id)) = parse_queue_key(&key) else {
                    continue;
                };
                if let Err(err) = self
                    .update_leaderboard_view(&lb_type, &mode, entity_id)
                    .await
                {
                    eprintln!("[leaderboard] update {key} error: {err}");
                }
                self.refresh_cache(&lb_type, &mode, entity_id).await;
            }

            if !self.update_queue.is_empty() {
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        }
    }

    fn cleanup_cache(&self) {
        let now = now_ms();
        self.cache.retain(|_, (_, ts)| now - *ts <= CACHE_TTL_MS);
    }

    fn queue_update(&self, key: &str) {
        self.update_queue.insert(key.to_string(), ());
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// JS buildPlayerEntries shape: falsy fields are omitted, allianceId /
/// allianceName always present with defaults.
fn build_player_object(
    id: i32,
    name: &str,
    picture: Option<&str>,
    alliance_id: Option<i32>,
    alliance_name: Option<&str>,
    equipped_flag: i32,
    discord: Option<&str>,
    pixels: i64,
) -> Value {
    let mut o = Map::new();
    o.insert("id".into(), json!(id));
    o.insert("name".into(), json!(name));
    if let Some(p) = picture.filter(|p| !p.is_empty()) {
        o.insert("picture".into(), json!(p));
    }
    if let Some(a) = alliance_id.filter(|a| *a != 0) {
        o.insert("allianceId".into(), json!(a));
    }
    if let Some(n) = alliance_name.filter(|n| !n.is_empty()) {
        o.insert("allianceName".into(), json!(n));
    }
    if equipped_flag != 0 {
        o.insert("equippedFlag".into(), json!(equipped_flag));
    }
    if let Some(d) = discord.filter(|d| !d.is_empty()) {
        o.insert("discord".into(), json!(d));
    }
    o.insert("pixelsPainted".into(), json!(pixels));
    Value::Object(o)
}

fn build_region_object(region: &EnrichRegionRow, pixels: i64) -> Value {
    json!({
        "id": region.id,
        "name": region.name,
        "pixelsPainted": pixels,
        "cityId": region.city_id,
        "number": region.number,
        "countryId": region.country_id,
        "lastLatitude": region.latitude,
        "lastLongitude": region.longitude,
    })
}

/// Filter zero-sum groups, then assign ranks 1..N (JS filters before mapping).
fn to_ranked_entries<I>(rows: I) -> Vec<(i32, i32, i32)>
where
    I: IntoIterator<Item = (i32, i64)>,
{
    rows.into_iter()
        .filter(|(_, painted)| *painted > 0)
        .enumerate()
        .map(|(i, (id, painted))| (id, i as i32 + 1, painted as i32))
        .collect()
}

/// DashMap key; entity 0 normalizes to None (JS `${entityId || "all"}`).
fn cache_key(lb_type: &str, mode: &str, entity_id: Option<i32>) -> (String, String, Option<i32>) {
    (
        lb_type.to_string(),
        mode.to_string(),
        entity_id.filter(|e| *e != 0),
    )
}

fn key_string(lb_type: &str, mode: &str, entity_id: Option<i32>) -> String {
    let entity = match entity_id.filter(|e| *e != 0) {
        Some(e) => e.to_string(),
        None => "all".to_string(),
    };
    format!("{lb_type}:{mode}:{entity}")
}

fn parse_queue_key(key: &str) -> Option<(String, String, Option<i32>)> {
    let mut parts = key.splitn(3, ':');
    let lb_type = parts.next()?.to_string();
    let mode = parts.next()?.to_string();
    let entity = parts.next()?;
    let entity_id = if entity == "all" {
        None
    } else {
        entity.parse::<i32>().ok()
    };
    Some((lb_type, mode, entity_id))
}

fn slice_items(items: &Value, limit: i64) -> Vec<Value> {
    items
        .as_array()
        .map(|a| a.iter().take(limit as usize).cloned().collect())
        .unwrap_or_default()
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
