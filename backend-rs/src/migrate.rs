//! One-shot data migration from the legacy Node.js backend
//! (Prisma + MariaDB/MySQL: model-name tables, camelCase columns)
//! into the Rust backend's PostgreSQL schema (snake_case).
//!
//! Usage:
//!   openplace-backend migrate-from-mysql "mysql://root:password@127.0.0.1:13306/openplace" [--force]
//!
//! Every table is streamed from MySQL in batches of 500 and upserted with
//! `INSERT ... ON CONFLICT (pk) DO UPDATE`, so the migration is idempotent
//! and can be re-run safely.

use chrono::{DateTime, Utc};
use futures_util::TryStreamExt;
use sqlx::mysql::{MySql, MySqlPoolOptions, MySqlRow};
use sqlx::postgres::PgPoolOptions;
use sqlx::query_builder::Separated;
use sqlx::{Postgres, QueryBuilder};
use uuid::Uuid;

use crate::config::Config;

const BATCH: usize = 500;

type Err = Box<dyn std::error::Error>;

// ---------------------------------------------------------------------------
// Generic value carrier so one typed row struct can feed both the MySQL
// SELECT (via FromRow) and the PG INSERT (via QueryBuilder::push_bind).
// ---------------------------------------------------------------------------

enum V<'a> {
    I16(i16),
    I32(i32),
    I64(i64),
    F64(f64),
    Bool(bool),
    Text(&'a str),
    TextOpt(Option<&'a str>),
    BytesOpt(Option<&'a [u8]>),
    I32Opt(Option<i32>),
    I64Opt(Option<i64>),
    F64Opt(Option<f64>),
    Ts(DateTime<Utc>),
    TsOpt(Option<DateTime<Utc>>),
    Uuid(Uuid),
}

impl V<'_> {
    fn bind<'a, 'qb, Sep: std::fmt::Display>(&'a self, b: &mut Separated<'qb, 'a, Postgres, Sep>) {
        match self {
            V::I16(v) => {
                b.push_bind(*v);
            }
            V::I32(v) => {
                b.push_bind(*v);
            }
            V::I64(v) => {
                b.push_bind(*v);
            }
            V::F64(v) => {
                b.push_bind(*v);
            }
            V::Bool(v) => {
                b.push_bind(*v);
            }
            V::Text(v) => {
                b.push_bind(*v);
            }
            V::TextOpt(v) => {
                b.push_bind(*v);
            }
            V::BytesOpt(v) => {
                b.push_bind(*v);
            }
            V::I32Opt(v) => {
                b.push_bind(*v);
            }
            V::I64Opt(v) => {
                b.push_bind(*v);
            }
            V::F64Opt(v) => {
                b.push_bind(*v);
            }
            V::Ts(v) => {
                b.push_bind(*v);
            }
            V::TsOpt(v) => {
                b.push_bind(*v);
            }
            V::Uuid(v) => {
                b.push_bind(*v);
            }
        }
    }
}

/// Row structs produce their values in the same order as the PG column list.
trait Cells {
    fn cells(&self) -> Vec<V<'_>>;
}

// ---------------------------------------------------------------------------
// Typed rows (one struct per legacy table).
// ---------------------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct AllianceRow {
    id: i32,
    name: String,
    description: Option<String>,
    hq_latitude: Option<f64>,
    hq_longitude: Option<f64>,
    pixels_painted: i32,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
struct UserRow {
    id: i32,
    name: String,
    registration_ip: Option<String>,
    last_ip: Option<String>,
    discord: Option<String>,
    discord_user_id: Option<String>,
    nickname: Option<String>,
    country: String,
    email: Option<String>,
    password_hash: String,
    banned: bool,
    verified: bool,
    suspension_reason: Option<String>,
    timeout_until: DateTime<Utc>,
    needs_phone_verification: bool,
    is_customer: bool,
    role: String,
    pixels_painted: i32,
    droplets: i32,
    max_charges: f64,
    current_charges: f64,
    charges_cooldown_ms: i32,
    charges_last_updated_at: DateTime<Utc>,
    extra_colors_bitmap: i32,
    flags_bitmap: Option<Vec<u8>>,
    equipped_flag: i32,
    show_last_pixel: bool,
    picture: Option<String>,
    level: f64,
    alliance_id: Option<i32>,
    alliance_role: String,
    alliance_joined_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
struct BannedUserRow {
    id: i32,
    user_id: i32,
    alliance_id: i32,
    created_at: DateTime<Utc>,
}

/// Legacy MySQL stores uuid PKs as CHAR(36) text; sqlx decodes that into
/// `uuid::fmt::Hyphenated` (plain `Uuid` expects the binary format there).
type UuidText = uuid::fmt::Hyphenated;

#[derive(sqlx::FromRow)]
struct AllianceInviteRow {
    id: UuidText,
    alliance_id: i32,
    created_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
struct FavoriteLocationRow {
    id: i32,
    user_id: i32,
    name: String,
    latitude: f64,
    longitude: f64,
}

#[derive(sqlx::FromRow)]
struct BannedIpRow {
    id: i32,
    cidr: String,
    ipv4_min: Option<i64>,
    ipv4_max: Option<i64>,
    ipv6_min: Option<Vec<u8>>,
    ipv6_max: Option<Vec<u8>>,
    suspension_reason: String,
    user_id: Option<i32>,
    created_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
struct TileRow {
    id: i64,
    season: i16,
    x: i32,
    y: i32,
    image_data: Option<Vec<u8>>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
struct PixelRow {
    id: i64,
    season: i16,
    tile_x: i32,
    tile_y: i32,
    x: i16,
    y: i16,
    color_id: i16,
    painted_by: i32,
    region_city_id: Option<i32>,
    region_country_id: Option<i32>,
    painted_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
struct RegionRow {
    id: i32,
    city_id: i32,
    name: String,
    number: i32,
    country_id: i32,
    latitude: f64,
    longitude: f64,
    population: i64,
}

#[derive(sqlx::FromRow)]
struct ProfilePictureRow {
    id: i32,
    user_id: i32,
    url: String,
}

#[derive(sqlx::FromRow)]
struct SessionRow {
    id: UuidText,
    user_id: i32,
    expires_at: DateTime<Utc>,
    created_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
struct PasswordResetTokenRow {
    id: UuidText,
    user_id: i32,
    expires_at: DateTime<Utc>,
    created_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
struct TicketRow {
    id: UuidText,
    user_id: i32,
    reported_user_id: i32,
    moderator_user_id: Option<i32>,
    latitude: f64,
    longitude: f64,
    zoom: f64,
    reason: String,
    notes: String,
    image: Option<Vec<u8>>,
    resolution: Option<String>,
    severe: bool,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
struct UserNoteRow {
    id: i32,
    user_id: i32,
    reported_user_id: i32,
    content: String,
    created_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
struct LeaderboardViewRow {
    id: i32,
    #[sqlx(rename = "type")]
    kind: String,
    mode: String,
    entity_id: Option<i32>,
    region_id: Option<i32>,
    rank: i32,
    pixels_painted: i32,
    last_updated: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
struct UserRegionStatsRow {
    id: i64,
    user_id: i32,
    region_city_id: Option<i32>,
    region_country_id: Option<i32>,
    alliance_id: Option<i32>,
    time_period: DateTime<Utc>,
    pixels_painted: i32,
    last_painted_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
struct UserRegionStatsDailyRow {
    id: i64,
    user_id: i32,
    region_city_id: Option<i32>,
    region_country_id: Option<i32>,
    alliance_id: Option<i32>,
    date: DateTime<Utc>,
    pixels_painted: i32,
    last_painted_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
struct NotificationRow {
    id: i32,
    user_id: i32,
    sending_user_id: i32,
    read: bool,
    icon: String,
    title: String,
    message: String,
    created_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
struct SystemNotificationRow {
    id: i32,
    icon: String,
    title: String,
    message: String,
    created_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
struct SystemNotificationReadRow {
    id: i32,
    system_notification_id: i32,
    user_id: i32,
    read_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Cells impls (order must match the PG column lists below).
// ---------------------------------------------------------------------------

impl Cells for AllianceRow {
    fn cells(&self) -> Vec<V<'_>> {
        vec![
            V::I32(self.id),
            V::Text(&self.name),
            V::TextOpt(self.description.as_deref()),
            V::F64Opt(self.hq_latitude),
            V::F64Opt(self.hq_longitude),
            V::I32(self.pixels_painted),
            V::Ts(self.created_at),
            V::Ts(self.updated_at),
        ]
    }
}

impl Cells for UserRow {
    fn cells(&self) -> Vec<V<'_>> {
        vec![
            V::I32(self.id),
            V::Text(&self.name),
            V::TextOpt(self.registration_ip.as_deref()),
            V::TextOpt(self.last_ip.as_deref()),
            V::TextOpt(self.discord.as_deref()),
            V::TextOpt(self.discord_user_id.as_deref()),
            V::TextOpt(self.nickname.as_deref()),
            V::Text(&self.country),
            V::TextOpt(self.email.as_deref()),
            V::Text(&self.password_hash),
            V::Bool(self.banned),
            V::Bool(self.verified),
            V::TextOpt(self.suspension_reason.as_deref()),
            V::Ts(self.timeout_until),
            V::Bool(self.needs_phone_verification),
            V::Bool(self.is_customer),
            V::Text(&self.role),
            V::I32(self.pixels_painted),
            V::I32(self.droplets),
            V::F64(self.max_charges),
            V::F64(self.current_charges),
            V::I32(self.charges_cooldown_ms),
            V::Ts(self.charges_last_updated_at),
            V::I32(self.extra_colors_bitmap),
            V::BytesOpt(self.flags_bitmap.as_deref()),
            V::I32(self.equipped_flag),
            V::Bool(self.show_last_pixel),
            V::TextOpt(self.picture.as_deref()),
            V::F64(self.level),
            V::I32Opt(self.alliance_id),
            V::Text(&self.alliance_role),
            V::TsOpt(self.alliance_joined_at),
            V::Ts(self.created_at),
            V::Ts(self.updated_at),
        ]
    }
}

impl Cells for BannedUserRow {
    fn cells(&self) -> Vec<V<'_>> {
        vec![
            V::I32(self.id),
            V::I32(self.user_id),
            V::I32(self.alliance_id),
            V::Ts(self.created_at),
        ]
    }
}

impl Cells for AllianceInviteRow {
    fn cells(&self) -> Vec<V<'_>> {
        vec![
            V::Uuid(self.id.into_uuid()),
            V::I32(self.alliance_id),
            V::Ts(self.created_at),
        ]
    }
}

impl Cells for FavoriteLocationRow {
    fn cells(&self) -> Vec<V<'_>> {
        vec![
            V::I32(self.id),
            V::I32(self.user_id),
            V::Text(&self.name),
            V::F64(self.latitude),
            V::F64(self.longitude),
        ]
    }
}

impl Cells for BannedIpRow {
    fn cells(&self) -> Vec<V<'_>> {
        vec![
            V::I32(self.id),
            V::Text(&self.cidr),
            V::I64Opt(self.ipv4_min),
            V::I64Opt(self.ipv4_max),
            V::BytesOpt(self.ipv6_min.as_deref()),
            V::BytesOpt(self.ipv6_max.as_deref()),
            V::Text(&self.suspension_reason),
            V::I32Opt(self.user_id),
            V::Ts(self.created_at),
        ]
    }
}

impl Cells for TileRow {
    fn cells(&self) -> Vec<V<'_>> {
        vec![
            V::I64(self.id),
            V::I16(self.season),
            V::I32(self.x),
            V::I32(self.y),
            V::BytesOpt(self.image_data.as_deref()),
            V::Ts(self.created_at),
            V::Ts(self.updated_at),
        ]
    }
}

impl Cells for PixelRow {
    fn cells(&self) -> Vec<V<'_>> {
        vec![
            V::I64(self.id),
            V::I16(self.season),
            V::I32(self.tile_x),
            V::I32(self.tile_y),
            V::I16(self.x),
            V::I16(self.y),
            V::I16(self.color_id),
            V::I32(self.painted_by),
            V::I32Opt(self.region_city_id),
            V::I32Opt(self.region_country_id),
            V::Ts(self.painted_at),
        ]
    }
}

impl Cells for RegionRow {
    fn cells(&self) -> Vec<V<'_>> {
        vec![
            V::I32(self.id),
            V::I32(self.city_id),
            V::Text(&self.name),
            V::I32(self.number),
            V::I32(self.country_id),
            V::F64(self.latitude),
            V::F64(self.longitude),
            V::I64(self.population),
        ]
    }
}

impl Cells for ProfilePictureRow {
    fn cells(&self) -> Vec<V<'_>> {
        vec![V::I32(self.id), V::I32(self.user_id), V::Text(&self.url)]
    }
}

impl Cells for SessionRow {
    fn cells(&self) -> Vec<V<'_>> {
        vec![
            V::Uuid(self.id.into_uuid()),
            V::I32(self.user_id),
            V::Ts(self.expires_at),
            V::Ts(self.created_at),
        ]
    }
}

impl Cells for PasswordResetTokenRow {
    fn cells(&self) -> Vec<V<'_>> {
        vec![
            V::Uuid(self.id.into_uuid()),
            V::I32(self.user_id),
            V::Ts(self.expires_at),
            V::Ts(self.created_at),
        ]
    }
}

impl Cells for TicketRow {
    fn cells(&self) -> Vec<V<'_>> {
        vec![
            V::Uuid(self.id.into_uuid()),
            V::I32(self.user_id),
            V::I32(self.reported_user_id),
            V::I32Opt(self.moderator_user_id),
            V::F64(self.latitude),
            V::F64(self.longitude),
            V::F64(self.zoom),
            V::Text(&self.reason),
            V::Text(&self.notes),
            V::BytesOpt(self.image.as_deref()),
            V::TextOpt(self.resolution.as_deref()),
            V::Bool(self.severe),
            V::Ts(self.created_at),
            V::Ts(self.updated_at),
        ]
    }
}

impl Cells for UserNoteRow {
    fn cells(&self) -> Vec<V<'_>> {
        vec![
            V::I32(self.id),
            V::I32(self.user_id),
            V::I32(self.reported_user_id),
            V::Text(&self.content),
            V::Ts(self.created_at),
        ]
    }
}

impl Cells for LeaderboardViewRow {
    fn cells(&self) -> Vec<V<'_>> {
        vec![
            V::I32(self.id),
            V::Text(&self.kind),
            V::Text(&self.mode),
            V::I32Opt(self.entity_id),
            V::I32Opt(self.region_id),
            V::I32(self.rank),
            V::I32(self.pixels_painted),
            V::Ts(self.last_updated),
        ]
    }
}

impl Cells for UserRegionStatsRow {
    fn cells(&self) -> Vec<V<'_>> {
        vec![
            V::I64(self.id),
            V::I32(self.user_id),
            V::I32Opt(self.region_city_id),
            V::I32Opt(self.region_country_id),
            V::I32Opt(self.alliance_id),
            V::Ts(self.time_period),
            V::I32(self.pixels_painted),
            V::Ts(self.last_painted_at),
        ]
    }
}

impl Cells for UserRegionStatsDailyRow {
    fn cells(&self) -> Vec<V<'_>> {
        vec![
            V::I64(self.id),
            V::I32(self.user_id),
            V::I32Opt(self.region_city_id),
            V::I32Opt(self.region_country_id),
            V::I32Opt(self.alliance_id),
            V::Ts(self.date),
            V::I32(self.pixels_painted),
            V::Ts(self.last_painted_at),
        ]
    }
}

impl Cells for NotificationRow {
    fn cells(&self) -> Vec<V<'_>> {
        vec![
            V::I32(self.id),
            V::I32(self.user_id),
            V::I32(self.sending_user_id),
            V::Bool(self.read),
            V::Text(&self.icon),
            V::Text(&self.title),
            V::Text(&self.message),
            V::Ts(self.created_at),
        ]
    }
}

impl Cells for SystemNotificationRow {
    fn cells(&self) -> Vec<V<'_>> {
        vec![
            V::I32(self.id),
            V::Text(&self.icon),
            V::Text(&self.title),
            V::Text(&self.message),
            V::Ts(self.created_at),
        ]
    }
}

impl Cells for SystemNotificationReadRow {
    fn cells(&self) -> Vec<V<'_>> {
        vec![
            V::I32(self.id),
            V::I32(self.system_notification_id),
            V::I32(self.user_id),
            V::Ts(self.read_at),
        ]
    }
}

// ---------------------------------------------------------------------------
// Migration driver.
// ---------------------------------------------------------------------------

/// Tables with `GENERATED ... AS IDENTITY` ids that need setval() after load.
const IDENTITY_TABLES: &[&str] = &[
    "users",
    "alliances",
    "alliance_banned_users",
    "favorite_locations",
    "banned_ips",
    "tiles",
    "pixels",
    "regions",
    "profile_pictures",
    "user_notes",
    "leaderboard_view",
    "notifications",
    "system_notifications",
    "system_notification_reads",
    "user_region_stats",
    "user_region_stats_daily",
];

pub async fn run(config: &Config, source: &str, force: bool) -> Result<(), Err> {
    println!("[migrate] connecting to MySQL source: {source}");
    let mysql = MySqlPoolOptions::new()
        .max_connections(4)
        .acquire_timeout(std::time::Duration::from_secs(30))
        .connect(source)
        .await?;

    let pg_pool = PgPoolOptions::new()
        .max_connections(4)
        .acquire_timeout(std::time::Duration::from_secs(10))
        .connect(&config.database_url)
        .await?;

    match sqlx::migrate!("./migrations").run(&pg_pool).await {
        Ok(_) => {}
        Err(sqlx::migrate::MigrateError::VersionMismatch(_)) => {
            eprintln!("[migrate] version mismatch — skipping (schema already applied)");
        }
        Err(e) => return Err(e.into()),
    }

    let (user_count,): (i64,) = sqlx::query_as("SELECT count(*) FROM users")
        .fetch_one(&pg_pool)
        .await?;
    if user_count > 2 && !force {
        return Err(format!(
            "[migrate] target database already has {user_count} users (expected at most the 2 \
             system users). Refusing to load into a non-empty database — pass --force to continue."
        )
        .into());
    }

    let mut pg = pg_pool.acquire().await?;
    sqlx::query("SET session_replication_role = replica")
        .execute(&mut *pg)
        .await?;

    let mut total: u64 = 0;

    // Legacy dependency order: Alliance, User, BannedUser, AllianceInvite,
    // FavoriteLocation, BannedIP, Tile, Pixel, Region, ProfilePicture, Session,
    // PasswordResetToken, Ticket, UserNote, LeaderboardView, UserRegionStats,
    // UserRegionStatsDaily, Notification, SystemNotification, SystemNotificationRead.
    total += migrate_table::<AllianceRow>(
        &mysql,
        &mut pg,
        "SELECT `id`, `name`, `description`, \
         `hqLatitude` AS hq_latitude, `hqLongitude` AS hq_longitude, \
         `pixelsPainted` AS pixels_painted, \
         `createdAt` AS created_at, `updatedAt` AS updated_at \
         FROM `Alliance` ORDER BY `id`",
        "alliances",
        "id",
        &[
            "id",
            "name",
            "description",
            "hq_latitude",
            "hq_longitude",
            "pixels_painted",
            "created_at",
            "updated_at",
        ],
    )
    .await?;

    total += migrate_table::<UserRow>(
        &mysql,
        &mut pg,
        "SELECT `id`, `name`, \
         `registrationIP` AS registration_ip, `lastIP` AS last_ip, \
         `discord`, `discordUserId` AS discord_user_id, `nickname`, `country`, `email`, \
         `passwordHash` AS password_hash, `banned`, `verified`, \
         `suspensionReason` AS suspension_reason, `timeoutUntil` AS timeout_until, \
         `needsPhoneVerification` AS needs_phone_verification, `isCustomer` AS is_customer, \
         `role`, `pixelsPainted` AS pixels_painted, `droplets`, \
         `maxCharges` AS max_charges, `currentCharges` AS current_charges, \
         `chargesCooldownMs` AS charges_cooldown_ms, \
         `chargesLastUpdatedAt` AS charges_last_updated_at, \
         `extraColorsBitmap` AS extra_colors_bitmap, `flagsBitmap` AS flags_bitmap, \
         `equippedFlag` AS equipped_flag, `showLastPixel` AS show_last_pixel, \
         `picture`, `level`, \
         `allianceId` AS alliance_id, `allianceRole` AS alliance_role, \
         `allianceJoinedAt` AS alliance_joined_at, \
         `createdAt` AS created_at, `updatedAt` AS updated_at \
         FROM `User` ORDER BY `id`",
        "users",
        "id",
        &[
            "id",
            "name",
            "registration_ip",
            "last_ip",
            "discord",
            "discord_user_id",
            "nickname",
            "country",
            "email",
            "password_hash",
            "banned",
            "verified",
            "suspension_reason",
            "timeout_until",
            "needs_phone_verification",
            "is_customer",
            "role",
            "pixels_painted",
            "droplets",
            "max_charges",
            "current_charges",
            "charges_cooldown_ms",
            "charges_last_updated_at",
            "extra_colors_bitmap",
            "flags_bitmap",
            "equipped_flag",
            "show_last_pixel",
            "picture",
            "level",
            "alliance_id",
            "alliance_role",
            "alliance_joined_at",
            "created_at",
            "updated_at",
        ],
    )
    .await?;

    total += migrate_table::<BannedUserRow>(
        &mysql,
        &mut pg,
        "SELECT `id`, `userId` AS user_id, `allianceId` AS alliance_id, \
         `createdAt` AS created_at \
         FROM `BannedUser` ORDER BY `id`",
        "alliance_banned_users",
        "id",
        &["id", "user_id", "alliance_id", "created_at"],
    )
    .await?;

    total += migrate_table::<AllianceInviteRow>(
        &mysql,
        &mut pg,
        "SELECT `id`, `allianceId` AS alliance_id, `createdAt` AS created_at \
         FROM `AllianceInvite` ORDER BY `id`",
        "alliance_invites",
        "id",
        &["id", "alliance_id", "created_at"],
    )
    .await?;

    total += migrate_table::<FavoriteLocationRow>(
        &mysql,
        &mut pg,
        "SELECT `id`, `userId` AS user_id, `name`, `latitude`, `longitude` \
         FROM `FavoriteLocation` ORDER BY `id`",
        "favorite_locations",
        "id",
        &["id", "user_id", "name", "latitude", "longitude"],
    )
    .await?;

    total += migrate_table::<BannedIpRow>(
        &mysql,
        &mut pg,
        "SELECT `id`, `cidr`, \
         CAST(`ipv4Min` AS SIGNED) AS ipv4_min, CAST(`ipv4Max` AS SIGNED) AS ipv4_max, \
         `ipv6Min` AS ipv6_min, `ipv6Max` AS ipv6_max, \
         `suspensionReason` AS suspension_reason, `userId` AS user_id, \
         `createdAt` AS created_at \
         FROM `BannedIP` ORDER BY `id`",
        "banned_ips",
        "id",
        &[
            "id",
            "cidr",
            "ipv4_min",
            "ipv4_max",
            "ipv6_min",
            "ipv6_max",
            "suspension_reason",
            "user_id",
            "created_at",
        ],
    )
    .await?;

    total += migrate_table::<TileRow>(
        &mysql,
        &mut pg,
        "SELECT CAST(`id` AS SIGNED) AS id, CAST(`season` AS SIGNED) AS season, \
         CAST(`x` AS SIGNED) AS x, CAST(`y` AS SIGNED) AS y, \
         `imageData` AS image_data, \
         `createdAt` AS created_at, `updatedAt` AS updated_at \
         FROM `Tile` ORDER BY `id`",
        "tiles",
        "id",
        &[
            "id",
            "season",
            "x",
            "y",
            "image_data",
            "created_at",
            "updated_at",
        ],
    )
    .await?;

    total += migrate_table::<PixelRow>(
        &mysql,
        &mut pg,
        "SELECT CAST(`id` AS SIGNED) AS id, CAST(`season` AS SIGNED) AS season, \
         CAST(`tileX` AS SIGNED) AS tile_x, CAST(`tileY` AS SIGNED) AS tile_y, \
         CAST(`x` AS SIGNED) AS x, CAST(`y` AS SIGNED) AS y, \
         CAST(`colorId` AS SIGNED) AS color_id, \
         `paintedBy` AS painted_by, \
         `regionCityId` AS region_city_id, `regionCountryId` AS region_country_id, \
         `paintedAt` AS painted_at \
         FROM `Pixel` ORDER BY `id`",
        "pixels",
        "id",
        &[
            "id",
            "season",
            "tile_x",
            "tile_y",
            "x",
            "y",
            "color_id",
            "painted_by",
            "region_city_id",
            "region_country_id",
            "painted_at",
        ],
    )
    .await?;

    total += migrate_table::<RegionRow>(
        &mysql,
        &mut pg,
        "SELECT `id`, `cityId` AS city_id, `name`, `number`, `countryId` AS country_id, \
         `latitude`, `longitude`, CAST(`population` AS SIGNED) AS population \
         FROM `Region` ORDER BY `id`",
        "regions",
        "id",
        &[
            "id",
            "city_id",
            "name",
            "number",
            "country_id",
            "latitude",
            "longitude",
            "population",
        ],
    )
    .await?;

    total += migrate_table::<ProfilePictureRow>(
        &mysql,
        &mut pg,
        "SELECT `id`, `userId` AS user_id, `url` \
         FROM `ProfilePicture` ORDER BY `id`",
        "profile_pictures",
        "id",
        &["id", "user_id", "url"],
    )
    .await?;

    total += migrate_table::<SessionRow>(
        &mysql,
        &mut pg,
        "SELECT `id`, `userId` AS user_id, `expiresAt` AS expires_at, `createdAt` AS created_at \
         FROM `Session` ORDER BY `id`",
        "sessions",
        "id",
        &["id", "user_id", "expires_at", "created_at"],
    )
    .await?;

    total += migrate_table::<PasswordResetTokenRow>(
        &mysql,
        &mut pg,
        "SELECT `id`, `userId` AS user_id, `expiresAt` AS expires_at, `createdAt` AS created_at \
         FROM `PasswordResetToken` ORDER BY `id`",
        "password_reset_tokens",
        "id",
        &["id", "user_id", "expires_at", "created_at"],
    )
    .await?;

    total += migrate_table::<TicketRow>(
        &mysql,
        &mut pg,
        "SELECT `id`, `userId` AS user_id, `reportedUserId` AS reported_user_id, \
         `moderatorUserId` AS moderator_user_id, \
         `latitude`, `longitude`, `zoom`, `reason`, `notes`, `image`, `resolution`, `severe`, \
         `createdAt` AS created_at, `updatedAt` AS updated_at \
         FROM `Ticket` ORDER BY `id`",
        "tickets",
        "id",
        &[
            "id",
            "user_id",
            "reported_user_id",
            "moderator_user_id",
            "latitude",
            "longitude",
            "zoom",
            "reason",
            "notes",
            "image",
            "resolution",
            "severe",
            "created_at",
            "updated_at",
        ],
    )
    .await?;

    total += migrate_table::<UserNoteRow>(
        &mysql,
        &mut pg,
        "SELECT `id`, `userId` AS user_id, `reportedUserId` AS reported_user_id, \
         `content`, `createdAt` AS created_at \
         FROM `UserNote` ORDER BY `id`",
        "user_notes",
        "id",
        &["id", "user_id", "reported_user_id", "content", "created_at"],
    )
    .await?;

    total += migrate_table::<LeaderboardViewRow>(
        &mysql,
        &mut pg,
        "SELECT `id`, `type`, `mode`, `entityId` AS entity_id, `regionId` AS region_id, \
         `rank`, `pixelsPainted` AS pixels_painted, `lastUpdated` AS last_updated \
         FROM `LeaderboardView` ORDER BY `id`",
        "leaderboard_view",
        "id",
        &[
            "id",
            "type",
            "mode",
            "entity_id",
            "region_id",
            "rank",
            "pixels_painted",
            "last_updated",
        ],
    )
    .await?;

    total += migrate_table::<UserRegionStatsRow>(
        &mysql,
        &mut pg,
        "SELECT CAST(`id` AS SIGNED) AS id, `userId` AS user_id, \
         `regionCityId` AS region_city_id, `regionCountryId` AS region_country_id, \
         `allianceId` AS alliance_id, `timePeriod` AS time_period, \
         `pixelsPainted` AS pixels_painted, `lastPaintedAt` AS last_painted_at \
         FROM `UserRegionStats` ORDER BY `id`",
        "user_region_stats",
        "id",
        &[
            "id",
            "user_id",
            "region_city_id",
            "region_country_id",
            "alliance_id",
            "time_period",
            "pixels_painted",
            "last_painted_at",
        ],
    )
    .await?;

    total += migrate_table::<UserRegionStatsDailyRow>(
        &mysql,
        &mut pg,
        "SELECT CAST(`id` AS SIGNED) AS id, `userId` AS user_id, \
         `regionCityId` AS region_city_id, `regionCountryId` AS region_country_id, \
         `allianceId` AS alliance_id, `date`, \
         `pixelsPainted` AS pixels_painted, `lastPaintedAt` AS last_painted_at \
         FROM `UserRegionStatsDaily` ORDER BY `id`",
        "user_region_stats_daily",
        "id",
        &[
            "id",
            "user_id",
            "region_city_id",
            "region_country_id",
            "alliance_id",
            "date",
            "pixels_painted",
            "last_painted_at",
        ],
    )
    .await?;

    total += migrate_table::<NotificationRow>(
        &mysql,
        &mut pg,
        "SELECT `id`, `userId` AS user_id, `sendingUserId` AS sending_user_id, `read`, \
         `icon`, `title`, `message`, `createdAt` AS created_at \
         FROM `Notification` ORDER BY `id`",
        "notifications",
        "id",
        &[
            "id",
            "user_id",
            "sending_user_id",
            "read",
            "icon",
            "title",
            "message",
            "created_at",
        ],
    )
    .await?;

    total += migrate_table::<SystemNotificationRow>(
        &mysql,
        &mut pg,
        "SELECT `id`, `icon`, `title`, `message`, `createdAt` AS created_at \
         FROM `SystemNotification` ORDER BY `id`",
        "system_notifications",
        "id",
        &["id", "icon", "title", "message", "created_at"],
    )
    .await?;

    total += migrate_table::<SystemNotificationReadRow>(
        &mysql,
        &mut pg,
        "SELECT `id`, `systemNotificationId` AS system_notification_id, `userId` AS user_id, \
         `readAt` AS read_at \
         FROM `SystemNotificationRead` ORDER BY `id`",
        "system_notification_reads",
        "id",
        &["id", "system_notification_id", "user_id", "read_at"],
    )
    .await?;

    // Realign identity sequences with the migrated (possibly explicit) ids.
    for table in IDENTITY_TABLES {
        sqlx::query(&format!(
            "SELECT setval(pg_get_serial_sequence('{table}', 'id'), \
             GREATEST((SELECT COALESCE(max(id), 1) FROM {table}), 1))"
        ))
        .execute(&mut *pg)
        .await?;
    }

    sqlx::query("SET session_replication_role = DEFAULT")
        .execute(&mut *pg)
        .await?;
    drop(pg);

    println!("[migrate] done: {total} rows total across 20 tables; running VACUUM ANALYZE...");
    sqlx::raw_sql("VACUUM ANALYZE").execute(&pg_pool).await?;

    mysql.close().await;
    pg_pool.close().await;
    println!("[migrate] finished");
    Ok(())
}

async fn migrate_table<T>(
    mysql: &sqlx::MySqlPool,
    pg: &mut sqlx::PgConnection,
    select_sql: &'static str,
    table: &str,
    pk: &str,
    columns: &[&str],
) -> Result<u64, Err>
where
    T: for<'r> sqlx::FromRow<'r, MySqlRow> + Cells + Send + Unpin,
{
    let mut total: u64 = 0;
    let mut stream = sqlx::query_as::<MySql, T>(select_sql).fetch(mysql);
    let mut batch: Vec<T> = Vec::with_capacity(BATCH);
    while let Some(row) = stream.try_next().await? {
        batch.push(row);
        if batch.len() == BATCH {
            upsert_batch(pg, table, pk, columns, &batch).await?;
            total += batch.len() as u64;
            batch.clear();
        }
    }
    if !batch.is_empty() {
        upsert_batch(pg, table, pk, columns, &batch).await?;
        total += batch.len() as u64;
    }
    println!("[migrate] {table}: {total} rows");
    Ok(total)
}

async fn upsert_batch<T: Cells>(
    pg: &mut sqlx::PgConnection,
    table: &str,
    pk: &str,
    columns: &[&str],
    rows: &[T],
) -> Result<(), Err> {
    let data: Vec<Vec<V<'_>>> = rows.iter().map(Cells::cells).collect();

    let mut qb: QueryBuilder<'_, Postgres> = QueryBuilder::new("INSERT INTO ");
    qb.push(table).push(" (");
    for (i, col) in columns.iter().enumerate() {
        if i > 0 {
            qb.push(", ");
        }
        qb.push(format!("\"{col}\""));
    }
    // push_values() prepends "VALUES " itself.
    qb.push(") ");
    qb.push_values(data.iter(), |mut b, cells| {
        for v in cells {
            v.bind(&mut b);
        }
    });
    qb.push(" ON CONFLICT (").push(format!("\"{pk}\""));
    qb.push(") DO UPDATE SET ");
    for (i, col) in columns.iter().enumerate() {
        if i > 0 {
            qb.push(", ");
        }
        qb.push(format!("\"{col}\" = EXCLUDED.\"{col}\""));
    }

    qb.build().execute(pg).await?;
    Ok(())
}
