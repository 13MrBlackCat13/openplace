use chrono::{DateTime, Local, TimeZone, Utc};
use dashmap::DashMap;
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;

/// One pending region-stats increment. Paint requests push these onto a
/// channel; a background flusher coalesces them per (user, region, day) and
/// writes at most two rows per key — the JS backend performs the same upserts
/// synchronously inside the request.
#[derive(Hash, Eq, PartialEq, Clone)]
struct StatKey {
    user_id: i32,
    city: Option<i32>,
    country: Option<i32>,
    alliance: Option<i32>,
}

pub struct StatsQueue {
    tx: mpsc::UnboundedSender<StatUpdate>,
    rx: tokio::sync::Mutex<mpsc::UnboundedReceiver<StatUpdate>>,
    acc: DashMap<StatKey, i64>,
}

#[derive(Clone)]
pub struct StatUpdate {
    pub user_id: i32,
    pub city: Option<i32>,
    pub country: Option<i32>,
    pub alliance: Option<i32>,
    pub delta: i64,
}

impl StatsQueue {
    pub fn new() -> Arc<Self> {
        let (tx, rx) = mpsc::unbounded_channel();
        Arc::new(Self {
            tx,
            rx: tokio::sync::Mutex::new(rx),
            acc: DashMap::new(),
        })
    }

    pub fn push(&self, update: StatUpdate) {
        let _ = self.tx.send(update);
    }

    /// Coalescing flusher. `city_invalidate_tx` receives every affected
    /// regionCityId for leaderboard invalidation.
    pub async fn run_flusher(
        queue: Arc<Self>,
        pool: PgPool,
        flush_ms: u64,
        city_invalidate_tx: mpsc::UnboundedSender<i32>,
    ) {
        let mut rx = queue.rx.lock().await;
        loop {
            // Drain pending updates into the accumulator.
            while let Ok(u) = rx.try_recv() {
                let key = StatKey {
                    user_id: u.user_id,
                    city: u.city,
                    country: u.country,
                    alliance: u.alliance,
                };
                *queue.acc.entry(key).or_insert(0) += u.delta;
            }
            if queue.acc.is_empty() {
                // Block until the first update arrives; adding it here is
                // essential — recv() consumes the message.
                match rx.recv().await {
                    Some(u) => {
                        let key = StatKey {
                            user_id: u.user_id,
                            city: u.city,
                            country: u.country,
                            alliance: u.alliance,
                        };
                        *queue.acc.entry(key).or_insert(0) += u.delta;
                    }
                    None => return,
                }
                continue;
            }
            tokio::time::sleep(std::time::Duration::from_millis(flush_ms)).await;
            // Keep draining while we sleep (loop handles the rest).
            while let Ok(u) = rx.try_recv() {
                let key = StatKey {
                    user_id: u.user_id,
                    city: u.city,
                    country: u.country,
                    alliance: u.alliance,
                };
                *queue.acc.entry(key).or_insert(0) += u.delta;
            }

            let day = local_midnight();
            let mut cities: Vec<i32> = Vec::new();
            for entry in queue.acc.iter() {
                let (key, delta) = (entry.key(), *entry.value());
                if delta == 0 {
                    continue;
                }
                if let Some(city) = key.city {
                    if !cities.contains(&city) {
                        cities.push(city);
                    }
                }
                for (table, period_col) in [
                    ("user_region_stats", "time_period"),
                    ("user_region_stats_daily", "date"),
                ] {
                    let res = upsert_stats(&pool, table, period_col, key, day, delta).await;
                    if let Err(err) = res {
                        eprintln!("[stats] upsert {table} error: {err}");
                    }
                }
            }
            queue.acc.clear();
            for city in cities {
                let _ = city_invalidate_tx.send(city);
            }
        }
    }
}

async fn upsert_stats(
    pool: &PgPool,
    table: &str,
    period_col: &str,
    key: &StatKey,
    day: DateTime<Utc>,
    delta: i64,
) -> sqlx::Result<()> {
    // Table/column names come from compile-time constants only.
    let sql = format!(
        r#"
        UPDATE {table} SET pixels_painted = pixels_painted + $6, last_painted_at = now()
        WHERE user_id = $1
          AND region_city_id IS NOT DISTINCT FROM $2
          AND region_country_id IS NOT DISTINCT FROM $3
          AND alliance_id IS NOT DISTINCT FROM $4
          AND {period_col} = $5
        "#,
    );
    let updated = sqlx::query(&sql)
        .bind(key.user_id)
        .bind(key.city)
        .bind(key.country)
        .bind(key.alliance)
        .bind(day)
        .bind(delta as i32)
        .execute(pool)
        .await?
        .rows_affected();
    if updated == 0 {
        let sql = format!(
            r#"
            INSERT INTO {table}
                (user_id, region_city_id, region_country_id, alliance_id, {period_col}, pixels_painted, last_painted_at)
            VALUES ($1, $2, $3, $4, $5, $6, now())
            ON CONFLICT (user_id, region_city_id, region_country_id, alliance_id, {period_col}) DO UPDATE
            SET pixels_painted = {table}.pixels_painted + EXCLUDED.pixels_painted,
                last_painted_at = now()
            "#,
        );
        sqlx::query(&sql)
            .bind(key.user_id)
            .bind(key.city)
            .bind(key.country)
            .bind(key.alliance)
            .bind(day)
            .bind(delta as i32)
            .execute(pool)
            .await?;
    }
    Ok(())
}

/// Local midnight of the current day (JS: new Date(y, m, d)).
pub fn local_midnight() -> DateTime<Utc> {
    let now = Local::now();
    let midnight = Local
        .with_ymd_and_hms(now.year(), now.month(), now.day(), 0, 0, 0)
        .single()
        .unwrap_or(now);
    midnight.with_timezone(&Utc)
}

use chrono::Datelike;

/// Start of a leaderboard period (JS getDateFilter):
/// today → local midnight; week → 7 days ago at local midnight;
/// month → 1st of month at local midnight; all-time → None.
pub fn period_start(mode: &str) -> Option<DateTime<Utc>> {
    let now = Local::now();
    match mode {
        "today" => Some(local_midnight()),
        "week" => {
            let week_ago = now - chrono::Duration::days(7);
            let midnight = Local
                .with_ymd_and_hms(week_ago.year(), week_ago.month(), week_ago.day(), 0, 0, 0)
                .single()
                .unwrap_or(week_ago);
            Some(midnight.with_timezone(&Utc))
        }
        "month" => {
            let midnight = Local
                .with_ymd_and_hms(now.year(), now.month(), 1, 0, 0, 0)
                .single()
                .unwrap_or(now);
            Some(midnight.with_timezone(&Utc))
        }
        _ => None,
    }
}

/// Aggregation helper used by leaderboard queries (kept here to avoid a
/// separate utils module): groups Vec<T> by key summing pixels.
pub fn group_sum<K: std::hash::Hash + Eq>(
    pairs: impl IntoIterator<Item = (K, i64)>,
) -> HashMap<K, i64> {
    let mut map: HashMap<K, i64> = HashMap::new();
    for (k, v) in pairs {
        *map.entry(k).or_insert(0) += v;
    }
    map
}
