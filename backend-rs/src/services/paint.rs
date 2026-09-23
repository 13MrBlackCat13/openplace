use axum::Json;
use chrono::Utc;
use serde_json::json;
use sqlx::Row;
use std::collections::HashMap;
use std::sync::Arc;

use crate::auth::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::services::tiles::TileStore;
use crate::services::user_cache::UserRuntime;
use crate::services::user_ops::{ban_user, get_ban};
use crate::state::AppState;
use crate::utils::bitmap::WplaceBitMap;
use crate::utils::colors::{check_color_unlocked, TILE_SIZE};
use crate::utils::levels::calculate_level;

const PIXEL_CHUNK: usize = 500;

/// POST /:season/pixel/:tileX/:tileY body.
#[derive(Debug, Clone)]
pub struct PaintInput {
    pub colors: Vec<i64>,
    pub coords: Vec<i64>,
}

pub async fn paint_pixels(
    state: &AppState,
    auth: &AuthUser,
    ip: &str,
    country_header: Option<&str>,
    tile_x: i32,
    tile_y: i32,
    input: PaintInput,
) -> ApiResult<Json<serde_json::Value>> {
    if input.colors.len() * 2 != input.coords.len() {
        return Err(ApiError::bad_request("Bad Request"));
    }

    let Some(runtime) = state.users.get_or_load(&state.pool, auth.id).await else {
        return Err(ApiError::Refresh);
    };

    // Suspended accounts (JS: force-ban + 451).
    if runtime.banned || runtime.timeout_until > Utc::now() {
        if runtime.banned {
            let _ = ban_user(state, auth.id, true, runtime.suspension_reason.clone()).await;
            return Err(ApiError::ban(runtime.suspension_reason.clone()));
        }
        return Err(ApiError::timeout(runtime.suspension_reason.clone()));
    }

    // IP ban / Tor check (JS: authService.getBan).
    let country = crate::services::user_ops::normalize_country(country_header);
    if let Some(ban) = get_ban(&state.pool, &state.config, ip, &country).await? {
        if state.config.ban_on_banned_ip {
            let _ = ban_user(state, auth.id, true, Some(ban.reason.clone())).await;
        }
        return Err(ApiError::ban(Some(ban.reason)));
    }

    // Build the valid pixel list: [x0,y0,x1,y1,...] → (x, y, colorId).
    let mut pixels: Vec<(i16, i16, u8)> = Vec::with_capacity(input.colors.len());
    for (i, &color) in input.colors.iter().enumerate() {
        if !(0..=63).contains(&color) {
            // JS: unpaid/unknown colors fail the unlock check with 403.
            return Err(ApiError::PaintForbidden(
                "attempted to paint with a colour that was not purchased.".to_string(),
            ));
        }
        if !check_color_unlocked(color as u8, runtime.extra_colors_bitmap) {
            return Err(ApiError::PaintForbidden(
                "attempted to paint with a colour that was not purchased.".to_string(),
            ));
        }
        let x = input.coords[i * 2];
        let y = input.coords[i * 2 + 1];
        let (x, y) = (x as i32, y as i32);
        if !(0..TILE_SIZE).contains(&x) || !(0..TILE_SIZE).contains(&y) {
            continue; // JS silently drops out-of-bounds coordinates
        }
        pixels.push((x as i16, y as i16, color as u8));
    }

    let painted = pixels.len() as i64;
    if painted == 0 {
        return Ok(Json(json!({ "painted": 0 })));
    }

    // Region resolution (cached KD-tree lookups) + charge cost.
    state.regions.ensure_loaded().await;
    let flags = WplaceBitMap::from_bytes(runtime.flags_bitmap.clone());
    let mut region_by_pixel: Vec<crate::services::region::RegionHit> =
        Vec::with_capacity(pixels.len());
    let mut total_cost = 0.0f64;
    for (x, y, _) in &pixels {
        let region = state
            .regions
            .for_pixel(tile_x, tile_y, *x as i32, *y as i32);
        let discounted = flags.get(region.country_id.max(0) as u32);
        total_cost += if discounted { 0.9 } else { 1.0 };
        region_by_pixel.push(region);
    }

    // Admin clear-pixels: all colors are 0 → delete instead of upsert.
    let is_clearing = runtime.role == "admin" && input.colors.iter().all(|&c| c == 0);

    if is_clearing {
        if !TileStore::is_cached(&state.tiles, tile_x, tile_y) {
            TileStore::ensure_row(&state.pool, tile_x, tile_y).await?;
        }
        for chunk in pixels.chunks(PIXEL_CHUNK) {
            let xs: Vec<i16> = chunk.iter().map(|p| p.0).collect();
            let ys: Vec<i16> = chunk.iter().map(|p| p.1).collect();
            sqlx::query(
                "DELETE FROM pixels p USING unnest($3::smallint[], $4::smallint[]) AS t(x, y) \
                 WHERE p.season = 0 AND p.tile_x = $1 AND p.tile_y = $2 AND p.x = t.x AND p.y = t.y",
            )
            .bind(tile_x)
            .bind(tile_y)
            .bind(&xs)
            .bind(&ys)
            .execute(&state.pool)
            .await?;
        }
        let zeros: Vec<(i16, i16, u8)> = pixels.iter().map(|(x, y, _)| (*x, *y, 0u8)).collect();
        state.tiles.apply(tile_x, tile_y, &zeros);
        return Ok(Json(json!({ "painted": painted })));
    }

    // --- Charges / level / rewards: one atomic statement, no SELECT FOR UPDATE.
    let updated =
        update_user_after_paint(state, auth.id, total_cost, painted, ip, &country).await?;
    let Some(new_runtime) = updated else {
        // Distinguish the failure cause with a fresh read.
        let Some(current) = state.users.refresh(&state.pool, auth.id).await else {
            return Err(ApiError::Refresh);
        };
        if current.banned {
            let _ = ban_user(state, auth.id, true, current.suspension_reason.clone()).await;
            return Err(ApiError::ban(current.suspension_reason.clone()));
        }
        if current.timeout_until > Utc::now() {
            return Err(ApiError::timeout(current.suspension_reason.clone()));
        }
        return Err(ApiError::PaintForbidden(
            "attempted to paint more pixels than there was charges.".to_string(),
        ));
    };
    let runtime: Arc<UserRuntime> = new_runtime;

    // Ensure the tile row exists, then upsert pixels in chunks.
    if !TileStore::is_cached(&state.tiles, tile_x, tile_y) {
        TileStore::ensure_row(&state.pool, tile_x, tile_y).await?;
    }
    for chunk in pixels.chunks(PIXEL_CHUNK) {
        let xs: Vec<i16> = chunk.iter().map(|p| p.0).collect();
        let ys: Vec<i16> = chunk.iter().map(|p| p.1).collect();
        let cs: Vec<i16> = chunk.iter().map(|p| p.2 as i16).collect();
        let rcs: Vec<Option<i32>> = chunk
            .iter()
            .zip(region_by_pixel.iter())
            .map(|(_, r)| Some(r.city_id))
            .collect();
        let rcos: Vec<Option<i32>> = chunk
            .iter()
            .zip(region_by_pixel.iter())
            .map(|(_, r)| Some(r.country_id))
            .collect();
        sqlx::query(
            "INSERT INTO pixels \
                 (season, tile_x, tile_y, x, y, color_id, painted_by, region_city_id, region_country_id, painted_at) \
             SELECT 0, $1, $2, t.x, t.y, t.c, $3, t.rc, t.rco, now() \
             FROM unnest($4::smallint[], $5::smallint[], $6::smallint[], $7::int[], $8::int[]) \
                  AS t(x, y, c, rc, rco) \
             ON CONFLICT (season, tile_x, tile_y, x, y) DO UPDATE SET \
                 color_id = EXCLUDED.color_id, \
                 painted_by = EXCLUDED.painted_by, \
                 painted_at = now(), \
                 region_city_id = EXCLUDED.region_city_id, \
                 region_country_id = EXCLUDED.region_country_id",
        )
        .bind(tile_x)
        .bind(tile_y)
        .bind(auth.id)
        .bind(&xs)
        .bind(&ys)
        .bind(&cs)
        .bind(&rcs)
        .bind(&rcos)
        .execute(&state.pool)
        .await?;
    }

    // In-RAM grid update + PNG re-encode + async blob persistence.
    state.tiles.apply(tile_x, tile_y, &pixels);

    // Alliance counters.
    if let Some(alliance_id) = runtime.alliance_id {
        let _ = sqlx::query(
            "UPDATE alliances SET pixels_painted = pixels_painted + $2, updated_at = now() WHERE id = $1",
        )
        .bind(alliance_id)
        .bind(painted as i32)
        .execute(&state.pool)
        .await;
    }

    // Region stats (write-behind) + leaderboard invalidation (async).
    let alliance_id = runtime.alliance_id;
    let mut grouped: HashMap<(Option<i32>, Option<i32>), i64> = HashMap::new();
    for r in &region_by_pixel {
        *grouped
            .entry((Some(r.city_id), Some(r.country_id)))
            .or_insert(0) += 1;
    }
    for ((city, country), delta) in grouped {
        state.stats.push(crate::services::stats::StatUpdate {
            user_id: auth.id,
            city,
            country,
            alliance: alliance_id,
            delta,
        });
    }

    Ok(Json(json!({ "painted": painted })))
}

/// The entire reward pipeline as a single atomic UPDATE:
/// lazy charge regen, sufficiency check, charge deduction, pixel/level/droplet
/// rewards, last_ip/country refresh — then full-row RETURNING to refresh the
/// user cache. Replaces the JS SELECT ... FOR UPDATE + retry loop.
async fn update_user_after_paint(
    state: &AppState,
    user_id: i32,
    cost: f64,
    painted: i64,
    ip: &str,
    country: &str,
) -> ApiResult<Option<Arc<UserRuntime>>> {
    let base = state.config.level_base_pixel;
    let exp = state.config.level_exponent;
    let sql = format!(
        r#"
        UPDATE users SET
          current_charges = GREATEST(0.0::double precision,
              LEAST(max_charges,
                  current_charges
                  + (EXTRACT(EPOCH FROM (now() - charges_last_updated_at)) * 1000.0
                     / GREATEST(charges_cooldown_ms, 1)))
              - $2),
          charges_last_updated_at = now(),
          pixels_painted = pixels_painted + $3,
          level = power((pixels_painted + $3)::double precision / $4, $5) + 1,
          max_charges = max_charges + $6::double precision * GREATEST(0::double precision,
              floor(power((pixels_painted + $3)::double precision / $4, $5) + 1)
              - floor(power(pixels_painted::double precision / $4, $5) + 1)),
          droplets = droplets + ($3 * $7)::int
              + CASE WHEN floor(power((pixels_painted + $3)::double precision / $4, $5) + 1)
                          > floor(power(pixels_painted::double precision / $4, $5) + 1)
                     THEN $8 ELSE 0 END,
          last_ip = COALESCE(NULLIF($9, ''), last_ip),
          country = COALESCE(NULLIF($10, ''), country),
          updated_at = now()
        WHERE id = $1
          AND NOT banned
          AND timeout_until <= now()
          AND LEAST(max_charges,
              current_charges
              + (EXTRACT(EPOCH FROM (now() - charges_last_updated_at)) * 1000.0
                 / GREATEST(charges_cooldown_ms, 1))) >= $2::double precision
        RETURNING {columns}
        "#,
        columns = UserRuntime::COLUMNS
    );
    let row = sqlx::query_as::<_, UserRuntime>(&sql)
        .bind(user_id)
        .bind(cost)
        .bind(painted as i32)
        .bind(base)
        .bind(exp)
        .bind(state.config.level_up_max_charges_reward)
        .bind(state.config.painted_droplets_reward as i32)
        .bind(state.config.level_up_droplets_reward as i32)
        .bind(ip)
        .bind(country)
        .fetch_optional(&state.pool)
        .await?;
    Ok(row.map(|u| {
        let arc = Arc::new(u);
        state.users.store(arc.clone());
        arc
    }))
}

/// Level display helper for /me (uses the transaction formula).
pub fn level_for(pixels: i32, state: &AppState) -> f64 {
    calculate_level(
        pixels as i64,
        state.config.level_base_pixel,
        state.config.level_exponent,
    )
}

/// Small helper for reading scalar rows in route code.
pub async fn scalar_i64(pool: &sqlx::PgPool, sql: &str) -> ApiResult<i64> {
    let row = sqlx::query(sql).fetch_one(pool).await?;
    Ok(row.try_get::<i64, _>(0).unwrap_or(0))
}
