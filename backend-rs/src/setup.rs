use std::io::Read;

use sqlx::postgres::PgPoolOptions;

use crate::config::Config;

async fn connect(config: &Config) -> Result<sqlx::PgPool, Box<dyn std::error::Error>> {
    let pool = PgPoolOptions::new()
        .max_connections(8)
        .acquire_timeout(std::time::Duration::from_secs(10))
        .connect(&config.database_url)
        .await?;
    sqlx::migrate!("./migrations").run(&pool).await?;
    Ok(pool)
}

/// Mirrors scripts/setup.ts: migrations + system users.
pub async fn run_setup(config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    let pool = connect(config).await?;
    for (id, name) in [(-1, "System"), (-2, "Deleted Account")] {
        sqlx::query(
            "INSERT INTO users (id, name, country, password_hash, banned) \
             VALUES ($1, $2, 'US', '!', true) \
             ON CONFLICT (id) DO UPDATE SET name = EXCLUDED.name",
        )
        .bind(id)
        .bind(name)
        .execute(&pool)
        .await?;
    }
    println!("[setup] done: migrations applied, system users seeded");
    Ok(())
}

/// Import a GeoNames dump (zip with a single TSV or a raw TSV).
/// Columns used: 0=geonameid, 1=name, 4=lat, 5=lon, 8=countryCode, 14=population.
pub async fn import_geonames(
    config: &Config,
    path: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let pool = connect(config).await?;
    let file = std::fs::File::open(path)?;
    let mut content: Vec<u8> = Vec::new();
    if path.ends_with(".zip") {
        let mut archive = zip::ZipArchive::new(file)?;
        let name = archive.file_names().next().ok_or("empty zip")?.to_string();
        let mut entry = archive.by_name(&name)?;
        entry.read_to_end(&mut content)?;
    } else {
        let mut file = file;
        file.read_to_end(&mut content)?;
    }
    let text = String::from_utf8_lossy(&content);
    let mut count = 0i64;
    let mut tx = pool.begin().await?;
    for line in text.lines() {
        let cols: Vec<&str> = line.split('\t').collect();
        if cols.len() < 15 {
            continue;
        }
        let city_id: i32 = match cols[0].parse() {
            Ok(v) => v,
            Err(_) => continue,
        };
        let name = cols[1];
        let lat: f64 = match cols[4].parse() {
            Ok(v) => v,
            Err(_) => continue,
        };
        let lon: f64 = match cols[5].parse() {
            Ok(v) => v,
            Err(_) => continue,
        };
        let country_id = crate::utils::country::country_id_by_code(cols[8]).unwrap_or(13);
        let population: i64 = cols[14].parse().unwrap_or(0);
        sqlx::query(
            "INSERT INTO regions (city_id, name, number, country_id, latitude, longitude, population) \
             VALUES ($1, $2, 1, $3, $4, $5, $6) \
             ON CONFLICT (city_id) DO UPDATE SET \
               name = EXCLUDED.name, country_id = EXCLUDED.country_id, \
               latitude = EXCLUDED.latitude, longitude = EXCLUDED.longitude, \
               population = EXCLUDED.population",
        )
        .bind(city_id)
        .bind(name)
        .bind(country_id)
        .bind(lat)
        .bind(lon)
        .bind(population)
        .execute(&mut *tx)
        .await?;
        count += 1;
        if count % 20_000 == 0 {
            tx.commit().await?;
            tx = pool.begin().await?;
            println!("[geonames] imported {count} rows");
        }
    }
    tx.commit().await?;
    println!("[geonames] imported {count} regions");
    Ok(())
}

pub async fn redraw_tiles(config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    let pool = connect(config).await?;
    let store = crate::services::tiles::TileStore::new(1024);
    let rows: Vec<(i32, i32)> =
        sqlx::query_as("SELECT DISTINCT tile_x, tile_y FROM pixels WHERE season = 0")
            .fetch_all(&pool)
            .await?;
    println!("[redraw] rebuilding {} tiles", rows.len());
    for (x, y) in rows {
        store.reload_from_db(&pool, x, y).await?;
    }
    println!("[redraw] done");
    Ok(())
}

pub async fn init_leaderboard(config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    let pool = connect(config).await?;
    let service = crate::services::leaderboard::LeaderboardService::new(pool);
    service.warmup().await;
    println!("[leaderboard] initialized");
    Ok(())
}

/// Synthetic benchmark data shared by the Node and Rust stacks.
pub async fn seed_bench(
    config: &Config,
    regions: i64,
    users: i64,
    tiles: i64,
) -> Result<(), Box<dyn std::error::Error>> {
    let pool = connect(config).await?;
    println!("[seed] regions={regions} users={users} tiles={tiles}");

    let mut tx = pool.begin().await?;
    for i in 1..=regions {
        let lat = (i % 170) as f64 - 85.0 + (i as f64 * 0.37 % 0.9);
        let lon = (i % 350) as f64 - 175.0 + (i as f64 * 0.61 % 0.9);
        sqlx::query(
            "INSERT INTO regions (city_id, name, number, country_id, latitude, longitude, population) \
             VALUES ($1, $2, 1, $3, $4, $5, $6) ON CONFLICT (city_id) DO NOTHING",
        )
        .bind(i as i32)
        .bind(format!("Bench Region {i}"))
        .bind(((i % 250) + 1) as i32)
        .bind(lat)
        .bind(lon)
        .bind(10000 + i)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;

    let mut tx = pool.begin().await?;
    // One shared bcrypt hash (cost 4) so benchmark users can log in.
    let bench_hash = bcrypt::hash("benchpass", 4).unwrap();
    for i in 1..=users {
        sqlx::query(
            "INSERT INTO users (id, name, country, password_hash, pixels_painted, level, max_charges, current_charges) \
             VALUES ($1, $2, 'US', $3, $4, 5, 1000000, 1000000) ON CONFLICT (id) DO NOTHING",
        )
        .bind(i as i32)
        .bind(format!("bench{i}"))
        .bind(&bench_hash)
        .bind((i * 37) as i32)
        .execute(&mut *tx)
        .await?;
    }
    sqlx::query(
        "INSERT INTO users (id, name, country, password_hash, banned) VALUES (-1, 'System', 'US', '!', true), (-2, 'Deleted Account', 'US', '!', true) \
         ON CONFLICT (id) DO NOTHING",
    )
    .execute(&mut *tx)
    .await?;
    // Explicit-id inserts must advance the identity sequence.
    sqlx::query("SELECT setval(pg_get_serial_sequence('users','id'), GREATEST((SELECT COALESCE(max(id), 1) FROM users), 1))")
        .execute(&pool)
        .await?;
    tx.commit().await?;

    use rand::Rng;
    let mut rng = rand::thread_rng();
    let tile_size = 1000i64;
    for t in 0..tiles {
        let tile_x = (t % 64) as i32;
        let tile_y = ((t / 64) % 64) as i32;
        sqlx::query("INSERT INTO tiles (season, x, y) VALUES (0, $1, $2) ON CONFLICT (season, x, y) DO NOTHING")
            .bind(tile_x)
            .bind(tile_y)
            .execute(&pool)
            .await?;
        let pixels_per_tile = 250_000i64;
        let mut tx = pool.begin().await?;
        let mut i = 0i64;
        while i < pixels_per_tile {
            let batch = 5000.min(pixels_per_tile - i);
            let mut xs = Vec::with_capacity(batch as usize);
            let mut ys = Vec::with_capacity(batch as usize);
            let mut cs = Vec::with_capacity(batch as usize);
            let mut ub = Vec::with_capacity(batch as usize);
            for _ in 0..batch {
                let x = rng.gen_range(0..tile_size) as i16;
                let y = rng.gen_range(0..tile_size) as i16;
                let color = rng.gen_range(1..64) as i16;
                let user = rng.gen_range(1..=users) as i32;
                xs.push(x);
                ys.push(y);
                cs.push(color);
                ub.push(user);
            }
            sqlx::query(
                "INSERT INTO pixels (season, tile_x, tile_y, x, y, color_id, painted_by, region_city_id, region_country_id, painted_at) \
                 SELECT 0, $1, $2, t.x, t.y, t.c, t.u, 1, 13, now() FROM \
                 unnest($3::smallint[], $4::smallint[], $5::smallint[], $6::int[]) AS t(x, y, c, u) \
                 ON CONFLICT (season, tile_x, tile_y, x, y) DO NOTHING",
            )
            .bind(tile_x)
            .bind(tile_y)
            .bind(&xs)
            .bind(&ys)
            .bind(&cs)
            .bind(&ub)
            .execute(&mut *tx)
            .await?;
            i += batch;
        }
        tx.commit().await?;
        println!("[seed] tile {tile_x},{tile_y} seeded");
    }
    println!("[seed] done");
    Ok(())
}

/// Port of scripts/import-ip-list.ts: one IP (`1.2.3.4`, `::1`) or CIDR
/// (`1.2.3.0/24`) per line; `#` lines are comments. Ranges are stored as
/// min/max pairs (uint32 for IPv4, 16 bytes for IPv6) like the JS version.
pub async fn import_ip_list(
    config: &Config,
    path: &str,
    reason: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    use std::net::IpAddr;
    let pool = connect(config).await?;
    let content = std::fs::read_to_string(path)?;
    let mut imported = 0i64;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let (addr_str, prefix) = match trimmed.split_once('/') {
            Some((a, p)) => (a, p.parse::<u8>().ok()),
            None => (trimmed, None),
        };
        let Ok(ip) = addr_str.parse::<IpAddr>() else {
            eprintln!("[ip-list] skipping invalid line: {trimmed}");
            continue;
        };
        let (cidr, v4_range, v6_range) = match ip {
            IpAddr::V4(v4) => {
                let bits = prefix.unwrap_or(32);
                if bits > 32 {
                    eprintln!("[ip-list] skipping invalid prefix: {trimmed}");
                    continue;
                }
                let mask = if bits == 0 {
                    0u32
                } else {
                    u32::MAX << (32 - bits)
                };
                let net = u32::from(v4) & mask;
                let start = net;
                let end = net | !mask;
                (
                    format!("{}/{}", std::net::Ipv4Addr::from(net), bits),
                    Some((start as i64, end as i64)),
                    None,
                )
            }
            IpAddr::V6(v6) => {
                let bits = prefix.unwrap_or(128);
                if bits > 128 {
                    eprintln!("[ip-list] skipping invalid prefix: {trimmed}");
                    continue;
                }
                let v6 = v6.to_canonical();
                match v6 {
                    IpAddr::V4(v4) => {
                        let mask = if bits.saturating_sub(96) == 0 {
                            0u32
                        } else {
                            u32::MAX << (32 - (bits - 96))
                        };
                        let net = u32::from(v4) & mask;
                        (
                            format!("{}/{}", std::net::Ipv4Addr::from(net), bits),
                            Some((net as i64, (net | !mask) as i64)),
                            None,
                        )
                    }
                    IpAddr::V6(v6) => {
                        let addr = u128::from(v6);
                        let mask = if bits == 0 {
                            0u128
                        } else {
                            u128::MAX << (128 - bits)
                        };
                        let net = addr & mask;
                        let start = net.to_be_bytes();
                        let end = (net | !mask).to_be_bytes();
                        (
                            format!("{}/{}", std::net::Ipv6Addr::from(net), bits),
                            None,
                            Some((start.to_vec(), end.to_vec())),
                        )
                    }
                }
            }
        };
        let exists: Option<(i32,)> =
            sqlx::query_as("SELECT id FROM banned_ips WHERE cidr = $1 LIMIT 1")
                .bind(&cidr)
                .fetch_optional(&pool)
                .await?;
        if exists.is_some() {
            continue;
        }
        let (v4min, v4max) = v4_range
            .map(|(a, b)| (Some(a), Some(b)))
            .unwrap_or((None, None));
        let (v6min, v6max) = v6_range
            .map(|(a, b)| (Some(a), Some(b)))
            .unwrap_or((None, None));
        sqlx::query(
            "INSERT INTO banned_ips (cidr, ipv4_min, ipv4_max, ipv6_min, ipv6_max, suspension_reason) \
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(&cidr)
        .bind(v4min)
        .bind(v4max)
        .bind(v6min)
        .bind(v6max)
        .bind(reason)
        .execute(&pool)
        .await?;
        imported += 1;
    }
    println!("[ip-list] imported {imported} ranges from {path}");
    Ok(())
}

/// Port of scripts/send-system-notification.ts.
pub async fn system_notification(
    config: &Config,
    title: &str,
    message: &str,
    icon: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let pool = connect(config).await?;
    let (id,): (i32,) = sqlx::query_as(
        "INSERT INTO system_notifications (icon, title, message) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(icon)
    .bind(title)
    .bind(message)
    .fetch_one(&pool)
    .await?;
    println!("[notification] created system notification #{id}");
    Ok(())
}
