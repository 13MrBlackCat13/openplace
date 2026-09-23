use std::collections::HashSet;
use std::io::Cursor;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, RwLock};

use dashmap::DashMap;
use sqlx::PgPool;
use tokio::sync::mpsc;

use crate::utils::colors::{palette_bytes, palette_trns, rgba_to_color_id, TILE_SIZE};

const TILE_PIXELS: usize = (TILE_SIZE * TILE_SIZE) as usize;

/// One tile kept fully in RAM: a 1 MB color-id grid plus the encoded indexed
/// PNG. The JS backend decodes + re-encodes the PNG blob on every paint batch
/// with sharp; here the grid is the source of truth in memory and the PNG is
/// re-encoded in ~1-3 ms with a fixed 65-color palette (no quantization).
pub struct TileEntry {
    grid: RwLock<Box<[u8]>>,
    png: RwLock<Arc<Vec<u8>>>,
    updated_unix: AtomicI64,
    dirty: AtomicBool,
    /// No DB row yet (blank tile served from memory only).
    phantom: AtomicBool,
    last_used: AtomicI64,
}

pub struct TileStore {
    map: DashMap<(i32, i32), Arc<TileEntry>>,
    dirty_tx: mpsc::UnboundedSender<(i32, i32)>,
    pub dirty_rx: tokio::sync::Mutex<mpsc::UnboundedReceiver<(i32, i32)>>,
    max_tiles: usize,
    empty_png: Arc<Vec<u8>>,
}

fn encode_png(grid: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(32 * 1024);
    let palette = palette_bytes();
    let trns = palette_trns();
    let mut encoder = png::Encoder::new(&mut out, TILE_SIZE as u32, TILE_SIZE as u32);
    encoder.set_color(png::ColorType::Indexed);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_palette(&palette);
    encoder.set_trns(trns);
    encoder.set_compression(png::Compression::Fast);
    encoder.set_filter(png::FilterType::Sub);
    let mut writer = encoder.write_header().expect("png header");
    writer.write_image_data(grid).expect("png data");
    writer.finish().expect("png finish");
    out
}

fn empty_grid() -> Box<[u8]> {
    vec![0u8; TILE_PIXELS].into_boxed_slice()
}

fn blank_png() -> Arc<Vec<u8>> {
    Arc::new(encode_png(&empty_grid()))
}

/// Decode any PNG (ours: indexed; legacy JS/sharp blobs: palette or RGBA)
/// into a color-id grid.
fn decode_png_to_grid(bytes: &[u8]) -> Option<Box<[u8]>> {
    let decoder = png::Decoder::new(Cursor::new(bytes));
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).ok()?;
    let (w, h) = (info.width as usize, info.height as usize);
    if w != TILE_SIZE as usize || h != TILE_SIZE as usize {
        return None;
    }
    let mut grid = empty_grid();
    match info.color_type {
        png::ColorType::Rgba => {
            for i in 0..TILE_PIXELS {
                let px = &buf[i * 4..i * 4 + 4];
                grid[i] = rgba_to_color_id(px[0], px[1], px[2], px[3]);
            }
        }
        png::ColorType::Rgb => {
            for i in 0..TILE_PIXELS {
                let px = &buf[i * 3..i * 3 + 3];
                grid[i] = rgba_to_color_id(px[0], px[1], px[2], 255);
            }
        }
        _ => return None,
    }
    Some(grid)
}

fn now_millis() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

impl TileStore {
    pub fn new(max_tiles: usize) -> Arc<Self> {
        let (dirty_tx, dirty_rx) = mpsc::unbounded_channel();
        Arc::new(Self {
            map: DashMap::new(),
            dirty_tx,
            dirty_rx: tokio::sync::Mutex::new(dirty_rx),
            max_tiles,
            empty_png: blank_png(),
        })
    }

    fn make_entry(grid: Box<[u8]>, png: Arc<Vec<u8>>, phantom: bool) -> Arc<TileEntry> {
        Arc::new(TileEntry {
            grid: RwLock::new(grid),
            png: RwLock::new(png),
            updated_unix: AtomicI64::new(chrono::Utc::now().timestamp()),
            dirty: AtomicBool::new(false),
            phantom: AtomicBool::new(phantom),
            last_used: AtomicI64::new(now_millis()),
        })
    }

    /// Lookup with DB fallback. Missing rows yield a blank in-memory entry
    /// marked `phantom` (cheap to serve, replaced on first paint).
    pub async fn entry(&self, pool: &PgPool, x: i32, y: i32) -> sqlx::Result<Arc<TileEntry>> {
        if let Some(entry) = self.map.get(&(x, y)) {
            entry.last_used.store(now_millis(), Ordering::Relaxed);
            return Ok(entry.clone());
        }
        let entry = match self.load_entry(pool, x, y).await? {
            Some(e) => e,
            None => Self::make_entry(empty_grid(), self.empty_png.clone(), true),
        };
        let entry = self.insert_and_evict((x, y), entry);
        Ok(entry)
    }

    /// Full load path: blob first (fast decode), falling back to rebuilding
    /// the grid from pixel rows; the (re)built blob is persisted.
    async fn load_entry(
        &self,
        pool: &PgPool,
        x: i32,
        y: i32,
    ) -> sqlx::Result<Option<Arc<TileEntry>>> {
        let row: Option<(Option<Vec<u8>>, chrono::DateTime<chrono::Utc>)> = sqlx::query_as(
            "SELECT image_data, updated_at FROM tiles WHERE season = 0 AND x = $1 AND y = $2",
        )
        .bind(x)
        .bind(y)
        .fetch_optional(pool)
        .await?;
        if let Some((Some(blob), _updated_at)) = row {
            if let Some(grid) = decode_png_to_grid(&blob) {
                return Ok(Some(Self::make_entry(grid, Arc::new(blob), false)));
            }
            // Unparseable blob — fall through to a pixel-row rebuild.
        }
        let grid = self.build_grid_from_db(pool, x, y).await?;
        if grid.is_none() {
            return Ok(None);
        }
        let grid = grid.unwrap();
        let png = Arc::new(encode_png(&grid));
        let entry = Self::make_entry(grid, png.clone(), false);
        let _ = self.dirty_tx.send((x, y));
        entry.last_used.store(now_millis(), Ordering::Relaxed);
        Ok(Some(entry))
    }

    async fn build_grid_from_db(
        &self,
        pool: &PgPool,
        x: i32,
        y: i32,
    ) -> sqlx::Result<Option<Box<[u8]>>> {
        let rows: Vec<(i16, i16, i16)> = sqlx::query_as(
            "SELECT x, y, color_id FROM pixels WHERE season = 0 AND tile_x = $1 AND tile_y = $2",
        )
        .bind(x)
        .bind(y)
        .fetch_all(pool)
        .await?;
        if rows.is_empty() {
            return Ok(None);
        }
        let mut grid = empty_grid();
        for (px, py, color) in rows {
            let (px, py) = (px as usize, py as usize);
            if px < TILE_SIZE as usize && py < TILE_SIZE as usize {
                grid[py * TILE_SIZE as usize + px] = color as u8;
            }
        }
        Ok(Some(grid))
    }

    fn insert_and_evict(&self, key: (i32, i32), entry: Arc<TileEntry>) -> Arc<TileEntry> {
        self.map.insert(key, entry.clone());
        if self.map.len() > self.max_tiles {
            // Evict the least-recently-used entries beyond the cap.
            let mut candidates: Vec<(i64, (i32, i32))> = self
                .map
                .iter()
                .map(|kv| (kv.value().last_used.load(Ordering::Relaxed), *kv.key()))
                .collect();
            candidates.sort_unstable();
            let excess = self.map.len() - self.max_tiles;
            for (_, k) in candidates.into_iter().take(excess) {
                if let Some((_, victim)) = self.map.remove(&k) {
                    if victim.dirty.load(Ordering::Acquire) {
                        // Never drop unsaved work: requeue for persistence.
                        victim.last_used.store(now_millis(), Ordering::Relaxed);
                        self.map.insert(k, victim);
                    }
                }
            }
        }
        entry
    }

    /// Apply painted pixels to the grid, re-encode the PNG and queue blob
    /// persistence. Called once per paint request (not per pixel).
    pub fn apply(&self, x: i32, y: i32, pixels: &[(i16, i16, u8)]) -> (Arc<Vec<u8>>, i64) {
        let entry = self.map.get(&(x, y)).map(|e| e.clone());
        let entry = match entry {
            Some(e) => e,
            None => {
                let e = Self::make_entry(empty_grid(), self.empty_png.clone(), false);
                self.insert_and_evict((x, y), e)
            }
        };
        let png = {
            let mut grid = entry.grid.write().unwrap();
            for (px, py, color) in pixels {
                let (px, py) = (*px as usize, *py as usize);
                if px < TILE_SIZE as usize && py < TILE_SIZE as usize {
                    grid[py * TILE_SIZE as usize + px] = *color;
                }
            }
            let encoded = Arc::new(encode_png(&grid));
            *entry.png.write().unwrap() = encoded.clone();
            encoded
        };
        let ts = chrono::Utc::now().timestamp();
        entry.updated_unix.store(ts, Ordering::Release);
        entry.phantom.store(false, Ordering::Release);
        entry.dirty.store(true, Ordering::Release);
        entry.last_used.store(now_millis(), Ordering::Relaxed);
        let _ = self.dirty_tx.send((x, y));
        (png, ts)
    }

    /// Blob persistence worker: coalesces dirty tiles and writes the newest
    /// PNG of each within `flush_ms`.
    pub async fn run_flusher(store: Arc<Self>, pool: PgPool, flush_ms: u64) {
        let mut rx = store.dirty_rx.lock().await;
        loop {
            let mut batch: HashSet<(i32, i32)> = HashSet::new();
            // Wait for the first dirty tile, then drain everything pending.
            if rx.recv().await.is_none() {
                return;
            }
            while let Ok(k) = rx.try_recv() {
                batch.insert(k);
            }
            tokio::time::sleep(std::time::Duration::from_millis(flush_ms)).await;
            for (x, y) in batch {
                let Some(entry) = store.map.get(&(x, y)).map(|e| e.clone()) else {
                    continue;
                };
                if !entry.dirty.swap(false, Ordering::AcqRel) {
                    continue;
                }
                let png = entry.png.read().unwrap().clone();
                let result = sqlx::query(
                    "UPDATE tiles SET image_data = $3, updated_at = now() \
                     WHERE season = 0 AND x = $1 AND y = $2",
                )
                .bind(x)
                .bind(y)
                .bind(&*png)
                .execute(&pool)
                .await;
                match result {
                    Ok(r) if r.rows_affected() == 0 => {
                        let _ = sqlx::query(
                            "INSERT INTO tiles (season, x, y, image_data) VALUES (0, $1, $2, $3) \
                             ON CONFLICT (season, x, y) DO UPDATE SET image_data = EXCLUDED.image_data, updated_at = now()",
                        )
                        .bind(x)
                        .bind(y)
                        .bind(&*png)
                        .execute(&pool)
                        .await;
                    }
                    Err(err) => {
                        entry.dirty.store(true, Ordering::Release);
                        eprintln!("[tiles] flush error {x},{y}: {err}");
                    }
                    _ => {}
                }
            }
        }
    }

    /// Startup/background reconciliation: tiles whose pixel rows are newer
    /// than the persisted blob are rebuilt and re-persisted.
    pub async fn reconcile(store: Arc<Self>, pool: PgPool) {
        let rows: Result<Vec<(i32, i32)>, sqlx::Error> = sqlx::query_as(
            "SELECT p.tile_x, p.tile_y FROM pixels p \
             LEFT JOIN tiles t ON t.season = p.season AND t.x = p.tile_x AND t.y = p.tile_y \
             WHERE p.season = 0 AND (t.id IS NULL OR t.updated_at < p.painted_at) \
             GROUP BY p.tile_x, p.tile_y LIMIT 500",
        )
        .fetch_all(&pool)
        .await;
        if let Ok(rows) = rows {
            for (x, y) in rows {
                let _ = store.reload_from_db(&pool, x, y).await;
            }
        }
    }

    pub async fn reload_from_db(&self, pool: &PgPool, x: i32, y: i32) -> sqlx::Result<()> {
        if let Some(grid) = self.build_grid_from_db(pool, x, y).await? {
            let png = Arc::new(encode_png(&grid));
            let entry = Self::make_entry(grid, png, false);
            let entry = self.insert_and_evict((x, y), entry);
            entry.dirty.store(true, Ordering::Release);
            let _ = self.dirty_tx.send((x, y));
        }
        Ok(())
    }

    /// Ensure the DB row exists before pixel upserts (JS: INSERT IGNORE).
    pub async fn ensure_row(pool: &PgPool, x: i32, y: i32) -> sqlx::Result<()> {
        sqlx::query("INSERT INTO tiles (season, x, y) VALUES (0, $1, $2) ON CONFLICT (season, x, y) DO NOTHING")
            .bind(x)
            .bind(y)
            .execute(pool)
            .await?;
        Ok(())
    }

    pub fn is_cached(&self, x: i32, y: i32) -> bool {
        self.map.contains_key(&(x, y))
    }

    /// Last-Modified from an entry (unix seconds).
    pub fn updated_at(entry: &TileEntry) -> i64 {
        entry.updated_unix.load(Ordering::Acquire)
    }

    /// Serve-side read of the encoded PNG plus its Last-Modified unix
    /// timestamp (routes/pixel_routes.rs); refreshes the LRU clock on access.
    pub fn serve(&self, entry: &TileEntry) -> (Arc<Vec<u8>>, i64) {
        entry.last_used.store(now_millis(), Ordering::Relaxed);
        (
            entry.png.read().unwrap().clone(),
            entry.updated_unix.load(Ordering::Acquire),
        )
    }
}
