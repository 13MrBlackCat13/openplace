use dashmap::DashMap;
use serde::Serialize;
use sqlx::PgPool;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};

use crate::utils::colors::TILE_SIZE;

pub const WORLD_PIXELS: f64 = TILE_SIZE as f64 * (1u64 << 11) as f64; // 1000 * 2^11

/// Fallback "region" when nothing is nearby (JS: openplace / Australia id 13).
pub fn fallback_region() -> RegionHit {
    RegionHit {
        id: 0,
        city_id: 0,
        name: "openplace".to_string(),
        number: 1,
        country_id: 13,
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct RegionHit {
    pub id: i32,
    pub city_id: i32,
    pub name: String,
    pub number: i32,
    pub country_id: i32,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct RegionItem {
    pub id: i32,
    pub city_id: i32,
    pub name: String,
    pub number: i32,
    pub country_id: i32,
    pub latitude: f64,
    pub longitude: f64,
    pub population: i64,
}

impl From<&RegionItem> for RegionHit {
    fn from(r: &RegionItem) -> Self {
        RegionHit {
            id: r.id,
            city_id: r.city_id,
            name: r.name.clone(),
            number: r.number,
            country_id: r.country_id,
        }
    }
}

/// Web Mercator world pixel → (latitude, longitude). Port of
/// RegionService.pixelsToCoordinates (canonical zoom 11, 1000px tiles).
pub fn pixels_to_lat_lon(tile_x: i32, tile_y: i32, x: i32, y: i32) -> (f64, f64) {
    let global_x = tile_x as f64 * TILE_SIZE as f64 + x as f64 + 0.5;
    let global_y = tile_y as f64 * TILE_SIZE as f64 + y as f64 + 0.5;
    let norm_x = global_x / WORLD_PIXELS;
    let norm_y = global_y / WORLD_PIXELS;
    let longitude = norm_x * 360.0 - 180.0;
    let latitude =
        (std::f64::consts::PI * (1.0 - 2.0 * norm_y)).sinh().atan() * 180.0 / std::f64::consts::PI;
    (latitude, longitude)
}

/// lat/lon → world pixel (used by alliance leaderboards for last-pixel coords).
pub fn lat_lon_to_world_pixels(lat: f64, lon: f64) -> (f64, f64) {
    let norm_x = (lon + 180.0) / 360.0;
    let lat_rad = lat.to_radians();
    let norm_y = (1.0 - (lat_rad.tan() + 1.0 / lat_rad.cos()).ln() / std::f64::consts::PI) / 2.0;
    (norm_x * WORLD_PIXELS, norm_y * WORLD_PIXELS)
}

// ---------------------------------------------------------------------------
// KD-tree over (lat, lon) — same structure the JS backend builds at runtime
// (alternating axes, squared euclid distance in degrees).
// ---------------------------------------------------------------------------

struct KdNode {
    item: RegionItem,
    left: i32,
    right: i32,
}

struct KdSnapshot {
    nodes: Vec<KdNode>,
}

fn build_rec(items: &mut Vec<RegionItem>, nodes: &mut Vec<KdNode>, depth: usize) -> i32 {
    if items.is_empty() {
        return -1;
    }
    let axis = depth % 2; // 0 = latitude, 1 = longitude
    items.sort_by(|a, b| {
        let (ca, cb) = if axis == 0 {
            (a.latitude, b.latitude)
        } else {
            (a.longitude, b.longitude)
        };
        ca.partial_cmp(&cb).unwrap_or(std::cmp::Ordering::Equal)
    });
    let mid = items.len() / 2;
    let pivot = items.remove(mid);
    let right_half: Vec<RegionItem> = items.split_off(mid);
    let idx = nodes.len();
    nodes.push(KdNode {
        item: pivot,
        left: -1,
        right: -1,
    });
    let left = build_rec(items, nodes, depth + 1);
    let right = build_rec(&mut { right_half }, nodes, depth + 1);
    let node = &mut nodes[idx];
    node.left = left;
    node.right = right;
    idx as i32
}

fn nearest_rec(
    nodes: &[KdNode],
    root: i32,
    lat: f64,
    lon: f64,
    depth: usize,
    best: &mut Option<(f64, RegionHit)>,
) {
    if root < 0 {
        return;
    }
    let node = &nodes[root as usize];
    let item = &node.item;
    let dlat = item.latitude - lat;
    let dlon = item.longitude - lon;
    let d = dlat * dlat + dlon * dlon;
    if best.is_none() || d < best.as_ref().unwrap().0 {
        *best = Some((d, RegionHit::from(item)));
    }
    let axis = depth % 2;
    let (item_coord, query_coord) = if axis == 0 {
        (item.latitude, lat)
    } else {
        (item.longitude, lon)
    };
    let (near, far) = if query_coord < item_coord {
        (node.left, node.right)
    } else {
        (node.right, node.left)
    };
    nearest_rec(nodes, near, lat, lon, depth + 1, best);
    let diff = query_coord - item_coord;
    if best
        .as_ref()
        .map(|(bd, _)| diff * diff < *bd)
        .unwrap_or(true)
    {
        nearest_rec(nodes, far, lat, lon, depth + 1, best);
    }
}

// ---------------------------------------------------------------------------
// Service
// ---------------------------------------------------------------------------

pub struct RegionService {
    pool: PgPool,
    tree: RwLock<Arc<KdSnapshot>>,
    cache: DashMap<(i32, i32, i16, i16), RegionHit>,
    loaded: AtomicBool,
}

impl RegionService {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            tree: RwLock::new(Arc::new(KdSnapshot { nodes: Vec::new() })),
            cache: DashMap::new(),
            loaded: AtomicBool::new(false),
        }
    }

    /// (Re)load all regions and rebuild the KD-tree.
    pub async fn load(&self) -> sqlx::Result<usize> {
        let rows: Vec<RegionItem> = sqlx::query_as(
            "SELECT id, city_id, name, number, country_id, latitude, longitude, population FROM regions",
        )
        .fetch_all(&self.pool)
        .await?;
        let count = rows.len();
        let mut items = rows;
        let mut nodes = Vec::with_capacity(items.len());
        build_rec(&mut items, &mut nodes, 0);
        *self.tree.write().unwrap() = Arc::new(KdSnapshot { nodes });
        self.cache.clear();
        self.loaded.store(true, Ordering::Release);
        Ok(count)
    }

    pub async fn ensure_loaded(&self) {
        if !self.loaded.load(Ordering::Acquire) {
            let _ = self.load().await;
        }
    }

    pub fn count(&self) -> usize {
        self.tree.read().unwrap().nodes.len()
    }

    /// Nearest region for a world pixel, with a per-pixel memo cache.
    pub fn for_pixel(&self, tile_x: i32, tile_y: i32, x: i32, y: i32) -> RegionHit {
        let key = (tile_x, tile_y, x as i16, y as i16);
        if let Some(hit) = self.cache.get(&key) {
            return hit.clone();
        }
        let (lat, lon) = pixels_to_lat_lon(tile_x, tile_y, x, y);
        let snapshot = self.tree.read().unwrap().clone();
        let mut best: Option<(f64, RegionHit)> = None;
        if !snapshot.nodes.is_empty() {
            nearest_rec(&snapshot.nodes, 0, lat, lon, 0, &mut best);
        }
        let hit = best.map(|(_, h)| h).unwrap_or_else(fallback_region);
        self.cache.insert(key, hit.clone());
        hit
    }

    pub fn invalidate_cache(&self) {
        self.cache.clear();
    }

    /// Region autocomplete — JS: contains query, ordered exact → startsWith →
    /// population desc → shorter name, top 10.
    pub async fn search(&self, query: &str) -> sqlx::Result<Vec<RegionItem>> {
        let rows: Vec<RegionItem> = sqlx::query_as(
            r#"SELECT id, city_id, name, number, country_id, latitude, longitude, population
               FROM regions
               WHERE position(lower($1) in lower(name)) > 0
               ORDER BY (lower(name) = lower($1)) DESC,
                        (name ILIKE $1 || '%') DESC,
                        population DESC,
                        char_length(name) ASC
               LIMIT 10"#,
        )
        .bind(query)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Region lookup by city_id (leaderboard enrichment).
    pub async fn by_city_id(&self, city_id: i32) -> sqlx::Result<Option<RegionItem>> {
        sqlx::query_as(
            "SELECT id, city_id, name, number, country_id, latitude, longitude, population \
             FROM regions WHERE city_id = $1",
        )
        .bind(city_id)
        .fetch_optional(&self.pool)
        .await
    }

    pub fn spawn_cache_cleanup(self: Arc<Self>) {
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(600)).await;
                self.invalidate_cache();
            }
        });
    }
}
