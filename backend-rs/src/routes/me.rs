//! Profile, store and account endpoints — port of src/routes/me.ts and
//! src/routes/store.ts (GET /me … POST /flag/equip/{id}).

use axum::extract::multipart::MultipartRejection;
use axum::extract::{Multipart, Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use base64::Engine as _;
use chrono::{SecondsFormat, Utc};
use serde_json::{json, Value};

use crate::auth::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::extract::FlexibleJson;
use crate::state::AppState;
use crate::utils::bitmap::WplaceBitMap;
use crate::utils::charges::regenerate;
use crate::utils::sanitize::{escape_html, name_contains_invalid_characters};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/me", get(me_profile).delete(delete_account))
        .route("/me/update", post(update_me))
        .route("/me/profile-pictures", get(profile_pictures))
        .route("/me/profile-picture", post(upload_profile_picture))
        .route("/me/profile-picture/change", post(change_profile_picture))
        .route("/me/sessions", delete(delete_sessions))
        .route("/purchase", post(purchase))
        .route("/flag/equip/{id}", post(flag_equip))
}

// ---------------------------------------------------------------------------
// GET /me
// ---------------------------------------------------------------------------

/// GET /me — user snapshot from UserCache with display-only charge
/// regeneration (the JS backend also persists the regen here; the Rust port
/// keeps GET /me read-only).
async fn me_profile(State(state): State<AppState>, auth: AuthUser) -> ApiResult<Json<Value>> {
    let user = state
        .users
        .get_or_load(&state.pool, auth.id)
        .await
        .ok_or(ApiError::UserNotFound)?;

    let now = Utc::now();
    let charges = regenerate(
        user.current_charges,
        user.max_charges,
        user.charges_cooldown_ms,
        user.charges_last_updated_at,
        now,
    );
    let config = &state.config;
    let cd = user.charges_cooldown_ms;
    let boost: Option<&str> = if cd == config.active_cooldown_ms as i32 {
        Some("active")
    } else if cd == config.booster_cooldown_ms as i32 {
        Some("booster")
    } else if cd == config.special_cooldown_ms as i32 {
        Some("special")
    } else {
        None
    };

    let favorites: Vec<(i32, String, f64, f64)> = sqlx::query_as(
        "SELECT id, name, latitude, longitude \
         FROM favorite_locations WHERE user_id = $1 ORDER BY id",
    )
    .bind(user.id)
    .fetch_all(&state.pool)
    .await?;
    let favorite_locations: Vec<Value> = favorites
        .into_iter()
        .map(|(id, name, latitude, longitude)| {
            json!({ "id": id, "name": name, "latitude": latitude, "longitude": longitude })
        })
        .collect();

    Ok(Json(json!({
        "maxFavoriteLocations": 50,
        "experiments": {
            "2025-09_pawtect": { "variant": "disabled" },
            "2025-09_discord_linking": { "enabled": false },
        },
        "id": user.id,
        "name": user.display_name(),
        "discord": user.discord.clone().unwrap_or_default(),
        "discordUserId": user.discord_user_id.clone(),
        "country": user.country.clone(),
        "banned": user.banned,
        "verified": user.verified,
        "suspensionReason": user.suspension_reason.clone(),
        "timeoutUntil": user.timeout_until.to_rfc3339_opts(SecondsFormat::Millis, true),
        "charges": {
            "cooldownMs": user.charges_cooldown_ms,
            "count": charges,
            "max": user.max_charges,
            "boost": boost,
        },
        "droplets": user.droplets,
        "equippedFlag": user.equipped_flag,
        "extraColorsBitmap": user.extra_colors_bitmap,
        "favoriteLocations": favorite_locations,
        "flagsBitmap": WplaceBitMap::from_bytes(user.flags_bitmap.clone()).to_base64(),
        "role": user.role.clone(),
        "isCustomer": user.is_customer,
        "level": user.level,
        "needsPhoneVerification": false,
        "picture": user.picture.clone().unwrap_or_default(),
        "pixelsPainted": user.pixels_painted,
        "showLastPixel": user.show_last_pixel,
        "allianceId": user.alliance_id,
        "allianceRole": user.alliance_role.clone(),
    })))
}

// ---------------------------------------------------------------------------
// POST /me/update
// ---------------------------------------------------------------------------

/// POST /me/update {name?, showLastPixel?, discord?}
///
/// JS types `name` as `nickname` all the way through
/// (validateUpdateUser + UserService.updateUser), so the validation messages
/// are the "The nickname …" variants — confirmed from the JS sources.
async fn update_me(
    State(state): State<AppState>,
    auth: AuthUser,
    FlexibleJson(body): FlexibleJson<Value>,
) -> ApiResult<Json<Value>> {
    // Route-level type checks (`typeof x !== "string"` → 400).
    let name: Option<&str> = match body.get("name") {
        None => None,
        Some(Value::String(s)) => Some(s.as_str()),
        Some(_) => return Err(ApiError::bad_request("Invalid name type")),
    };
    let show_last_pixel: Option<bool> = match body.get("showLastPixel") {
        None => None,
        Some(Value::Bool(b)) => Some(*b),
        Some(_) => return Err(ApiError::bad_request("Invalid showLastPixel type")),
    };
    let discord: Option<&str> = match body.get("discord") {
        None => None,
        Some(Value::String(s)) => Some(s.as_str()),
        Some(_) => return Err(ApiError::bad_request("Invalid discord type")),
    };

    // validateUpdateUser — the route passes `name` as `nickname`, so the
    // nickname branch applies: empty → invalid characters → too long.
    if let Some(n) = name {
        let trimmed = n.trim();
        if trimmed.is_empty() {
            return Err(ApiError::bad_request("The nickname cannot be empty"));
        }
        if name_contains_invalid_characters(trimmed) {
            return Err(ApiError::bad_request(
                "The nickname contains invalid characters (HTML/script tags not allowed)",
            ));
        }
        if trimmed.chars().count() > 16 {
            return Err(ApiError::bad_request(
                "The nickname has more than 16 characters",
            ));
        }
    }
    if let Some(d) = discord {
        if !d.is_empty() {
            let trimmed = d.trim();
            if trimmed.chars().count() > 32 {
                return Err(ApiError::bad_request(
                    "The discord has more than 32 characters",
                ));
            }
            // JS: /^[\w.]+$/ — \w is ASCII there.
            if !trimmed.is_empty()
                && !trimmed
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
            {
                return Err(ApiError::bad_request("Invalid discord username format"));
            }
        }
    }

    // UserService.updateUser — a linked Discord account cannot be renamed
    // (JS throws a plain Error → 500).
    if let Some(d) = discord {
        let user = state
            .users
            .get_or_load(&state.pool, auth.id)
            .await
            .ok_or(ApiError::Internal)?;
        let linked = user
            .discord_user_id
            .as_deref()
            .is_some_and(|id| !id.is_empty());
        if linked && Some(d) != user.discord.as_deref() {
            return Err(ApiError::Internal);
        }
    }

    // Persist: nickname/discord go through sanitizeInput (escape then trim);
    // an emptied discord clears the column.
    let mut qb = sqlx::QueryBuilder::<sqlx::Postgres>::new("UPDATE users SET updated_at = now()");
    if let Some(n) = name {
        let sanitized = escape_html(n).trim().to_string();
        qb.push(", nickname = ").push_bind(sanitized);
    }
    if let Some(v) = show_last_pixel {
        qb.push(", show_last_pixel = ").push_bind(v);
    }
    if let Some(d) = discord {
        let sanitized = escape_html(d).trim().to_string();
        if sanitized.is_empty() {
            qb.push(", discord = NULL");
        } else {
            qb.push(", discord = ").push_bind(sanitized);
        }
    }
    qb.push(" WHERE id = ").push_bind(auth.id);
    qb.build().execute(&state.pool).await?;
    state.users.refresh(&state.pool, auth.id).await;

    Ok(Json(json!({ "success": true })))
}

// ---------------------------------------------------------------------------
// GET /me/profile-pictures
// ---------------------------------------------------------------------------

async fn profile_pictures(State(state): State<AppState>, auth: AuthUser) -> ApiResult<Json<Value>> {
    let rows: Vec<(i32, String)> =
        sqlx::query_as("SELECT id, url FROM profile_pictures WHERE user_id = $1 ORDER BY id DESC")
            .bind(auth.id)
            .fetch_all(&state.pool)
            .await?;
    Ok(Json(json!(rows
        .into_iter()
        .map(|(id, url)| json!({ "id": id, "url": url }))
        .collect::<Vec<_>>())))
}

// ---------------------------------------------------------------------------
// POST /me/profile-picture (multipart)
// ---------------------------------------------------------------------------

const ALLOWED_MIME: [&str; 4] = ["image/jpeg", "image/png", "image/gif", "image/webp"];
const MAX_PICTURE_BYTES: usize = 2 * 1024 * 1024; // multer limit
const PICTURE_PRICE_DROPLETS: i32 = 20_000;

fn mime_for_extension(file_name: &str) -> Option<&'static str> {
    let dot = file_name.rfind('.')?;
    match file_name[dot..].to_ascii_lowercase().as_str() {
        ".jpg" | ".jpeg" => Some("image/jpeg"),
        ".png" => Some("image/png"),
        ".gif" => Some("image/gif"),
        ".webp" => Some("image/webp"),
        _ => None,
    }
}

/// JS validateImageContent — magic bytes must match the declared type
/// (WebP additionally needs "WEBP" at offset 8).
fn content_matches(buffer: &[u8], mime: &str) -> bool {
    match mime {
        "image/jpeg" => buffer.starts_with(&[0xFF, 0xD8, 0xFF]),
        "image/png" => buffer.starts_with(&[0x89, 0x50, 0x4E, 0x47]),
        "image/gif" => buffer.starts_with(&[0x47, 0x49, 0x46]),
        "image/webp" => {
            buffer.len() >= 12 && buffer.starts_with(b"RIFF") && &buffer[8..12] == b"WEBP"
        }
        _ => false,
    }
}

async fn upload_profile_picture(
    State(state): State<AppState>,
    auth: AuthUser,
    multipart: Result<Multipart, MultipartRejection>,
) -> ApiResult<Response> {
    let mut multipart = multipart.map_err(|_| ApiError::Internal)?;

    // multer runs before the handler: buffer the "image" part (memoryStorage)
    // and enforce the 2MB limit up front.
    let mut image: Option<(Option<String>, Option<String>, Vec<u8>)> = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| ApiError::Internal)?
    {
        if field.name() != Some("image") {
            continue;
        }
        let content_type = field.content_type().map(str::to_string);
        let file_name = field.file_name().map(str::to_string);
        let bytes = field
            .bytes()
            .await
            .map_err(|_| ApiError::Internal)?
            .to_vec();
        if bytes.len() > MAX_PICTURE_BYTES {
            return Err(ApiError::bad_request("Image file too large (max 2MB)"));
        }
        image = Some((content_type, file_name, bytes));
        break;
    }

    // Effective MIME: declared content type first, then the file extension
    // (blob uploads without a usable extension still work).
    let ct_mime = image
        .as_ref()
        .and_then(|(ct, _, _)| ct.as_deref())
        .and_then(|ct| ALLOWED_MIME.iter().find(|m| **m == ct).copied());
    let ext_mime = image
        .as_ref()
        .and_then(|(_, name, _)| name.as_deref())
        .and_then(mime_for_extension);
    let Some(mime) = ct_mime.or(ext_mime) else {
        return Err(ApiError::bad_request(
            "Only image files (JPG, PNG, GIF, WebP) are allowed",
        ));
    };

    // The JS handler reads droplets before touching the file.
    let droplets: Option<(i32,)> = sqlx::query_as("SELECT droplets FROM users WHERE id = $1")
        .bind(auth.id)
        .fetch_optional(&state.pool)
        .await?;
    let Some((droplets,)) = droplets else {
        return Err(ApiError::UserNotFound);
    };
    if droplets < PICTURE_PRICE_DROPLETS {
        // JS: res.status(400).json(createErrorResponse("You do not have
        // enough droplets.", 403)) — HTTP 400 with a 403 in the body.
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "You do not have enough droplets.", "status": 403 })),
        )
            .into_response());
    }

    let Some((_, _, bytes)) = image else {
        return Err(ApiError::bad_request("Image file is required"));
    };
    if !content_matches(&bytes, mime) {
        return Err(ApiError::bad_request("Invalid image file content"));
    }

    let base64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    if base64.is_empty() {
        return Err(ApiError::bad_request("Invalid file data"));
    }
    let picture_url = format!("data:{mime};base64,{base64}");
    // UserService.updateProfilePicture — ~100KB limit for the data URL.
    if picture_url.len() > 100_000 {
        return Err(ApiError::bad_request("Picture URL too long"));
    }

    let mut tx = state.pool.begin().await?;
    let (picture_id,): (i32,) =
        sqlx::query_as("INSERT INTO profile_pictures (user_id, url) VALUES ($1, $2) RETURNING id")
            .bind(auth.id)
            .bind(&picture_url)
            .fetch_one(&mut *tx)
            .await?;
    sqlx::query("UPDATE users SET picture = $2, droplets = droplets - $3 WHERE id = $1")
        .bind(auth.id)
        .bind(&picture_url)
        .bind(PICTURE_PRICE_DROPLETS)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    state.users.refresh(&state.pool, auth.id).await;

    Ok(Json(json!({
        "success": true,
        "pictureId": picture_id,
        "pictureUrl": picture_url,
    }))
    .into_response())
}

// ---------------------------------------------------------------------------
// POST /me/profile-picture/change
// ---------------------------------------------------------------------------

async fn change_profile_picture(
    State(state): State<AppState>,
    auth: AuthUser,
    FlexibleJson(body): FlexibleJson<Value>,
) -> ApiResult<Json<Value>> {
    let picture_id = body.get("pictureId");
    if picture_id.is_none() || matches!(picture_id, Some(Value::Null)) {
        // Empty payload → clear the picture.
        sqlx::query("UPDATE users SET picture = NULL WHERE id = $1")
            .bind(auth.id)
            .execute(&state.pool)
            .await?;
        state.users.refresh(&state.pool, auth.id).await;
        return Ok(Json(json!({ "success": true })));
    }

    let pid = match picture_id {
        Some(Value::Number(n)) => n.as_i64(),
        _ => None,
    };
    let Some(pid) = pid.filter(|v| *v > 0 && *v <= i32::MAX as i64) else {
        return Err(ApiError::bad_request("Invalid picture ID"));
    };

    let owned: Option<(String,)> =
        sqlx::query_as("SELECT url FROM profile_pictures WHERE id = $1 AND user_id = $2")
            .bind(pid as i32)
            .bind(auth.id)
            .fetch_optional(&state.pool)
            .await?;
    let Some((url,)) = owned else {
        return Err(ApiError::bad_request(
            "Profile picture not found or access denied",
        ));
    };
    sqlx::query("UPDATE users SET picture = $2 WHERE id = $1")
        .bind(auth.id)
        .bind(url)
        .execute(&state.pool)
        .await?;
    state.users.refresh(&state.pool, auth.id).await;
    Ok(Json(json!({ "success": true })))
}

// ---------------------------------------------------------------------------
// DELETE /me/sessions
// ---------------------------------------------------------------------------

async fn delete_sessions(State(state): State<AppState>, auth: AuthUser) -> ApiResult<Json<Value>> {
    sqlx::query("DELETE FROM sessions WHERE user_id = $1")
        .bind(auth.id)
        .execute(&state.pool)
        .await?;
    state.sessions.remove_all_for_user(auth.id);
    Ok(Json(json!({ "success": true })))
}

// ---------------------------------------------------------------------------
// DELETE /me
// ---------------------------------------------------------------------------

async fn delete_account(
    State(state): State<AppState>,
    auth: AuthUser,
    FlexibleJson(body): FlexibleJson<Value>,
) -> ApiResult<Json<Value>> {
    let user = state
        .users
        .get_or_load(&state.pool, auth.id)
        .await
        .ok_or(ApiError::UserNotFound)?;

    // JS: confirmText must equal the stored nickname (null nickname fails).
    let confirm = body.get("confirmText").and_then(Value::as_str);
    let confirmed = matches!(
        (confirm, user.nickname.as_deref()),
        (Some(c), Some(n)) if c == n
    );
    if !confirmed {
        return Err(ApiError::bad_request("Invalid confirm text"));
    }

    let mut tx = state.pool.begin().await?;
    sqlx::query("DELETE FROM profile_pictures WHERE user_id = $1")
        .bind(auth.id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM sessions WHERE user_id = $1")
        .bind(auth.id)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "UPDATE users SET nickname = 'Deleted Account', discord = NULL, \
         discord_user_id = NULL, role = 'deleted' WHERE id = $1",
    )
    .bind(auth.id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    state.sessions.remove_all_for_user(auth.id);
    state.users.invalidate(auth.id);
    Ok(Json(json!({ "success": true })))
}

// ---------------------------------------------------------------------------
// POST /purchase  (src/routes/store.ts)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq)]
enum StoreItem {
    MaxCharges,
    PaintCharges,
    Color,
    Flag,
}

/// JS Number() for the string inputs the API can carry.
fn js_number(value: &str) -> f64 {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return 0.0;
    }
    trimmed.parse::<f64>().unwrap_or(f64::NAN)
}

async fn purchase(
    State(state): State<AppState>,
    auth: AuthUser,
    FlexibleJson(body): FlexibleJson<Value>,
) -> ApiResult<Json<Value>> {
    let product = body.get("product");

    // JS: !product || !product.id → "Bad Request" (falsy ids included).
    let pid_f64: Option<f64> = match product.and_then(|p| p.get("id")) {
        Some(Value::Number(n)) => n.as_f64(),
        Some(Value::String(s)) => Some(js_number(s)),
        _ => None,
    };
    let Some(pid) = pid_f64.filter(|v| v.fract() == 0.0 && *v > 0.0 && *v <= i32::MAX as f64)
    else {
        return Err(ApiError::bad_request("Bad Request"));
    };
    let pid = pid as i32;

    let (price, item) = match pid {
        70 => (500.0, StoreItem::MaxCharges),
        80 => (500.0, StoreItem::PaintCharges),
        100 => (2000.0, StoreItem::Color),
        110 => (20000.0, StoreItem::Flag),
        _ => return Err(ApiError::bad_request("Invalid item")),
    };

    let user = state
        .users
        .get_or_load(&state.pool, auth.id)
        .await
        .ok_or(ApiError::Unauthorized)?;

    // JS: product.amount ?? 1, then strict integer ≥ 1 validation.
    let amount = match product.and_then(|p| p.get("amount")) {
        None | Some(Value::Null) => 1.0,
        Some(Value::Number(n)) => n.as_f64().unwrap_or(f64::NAN),
        Some(_) => f64::NAN,
    };
    if !amount.is_finite() || amount.fract() != 0.0 || amount < 1.0 {
        return Err(ApiError::bad_request("Bad Request"));
    }

    let total_cost = price * amount;
    if (user.droplets as f64) < total_cost {
        return Err(ApiError::forbidden("Forbidden"));
    }

    // JS: if (product.variant) — falsy variants skip the unlock entirely
    // (only the droplets are charged).
    let variant: Option<f64> = if matches!(item, StoreItem::Color | StoreItem::Flag) {
        match product.and_then(|p| p.get("variant")) {
            Some(Value::Number(n)) => n.as_f64(),
            Some(Value::String(s)) => Some(js_number(s)),
            Some(Value::Bool(true)) => Some(1.0),
            _ => None,
        }
    } else {
        None
    };

    let mut extra_colors = user.extra_colors_bitmap;
    let mut flags_bitmap = user.flags_bitmap.clone();
    if let Some(v) = variant {
        match item {
            StoreItem::Color => {
                if v.fract() != 0.0 || !(32.0..=63.0).contains(&v) {
                    return Err(ApiError::bad_request("Bad Request"));
                }
                extra_colors |= 1i32 << (v as i32 - 32);
            }
            StoreItem::Flag => {
                if v.fract() != 0.0 || !(1.0..=251.0).contains(&v) {
                    return Err(ApiError::bad_request("Bad Request"));
                }
                let mut bitmap = WplaceBitMap::from_bytes(flags_bitmap);
                bitmap.set(v as u32, true);
                flags_bitmap = Some(bitmap.bytes);
            }
            _ => {}
        }
    }

    let now = Utc::now();
    let new_droplets = (user.droplets as f64 - total_cost) as i32;
    let max_charges = if item == StoreItem::MaxCharges {
        user.max_charges + 5.0 * amount
    } else {
        user.max_charges
    };
    let (current_charges, charges_last_updated_at) = if item == StoreItem::PaintCharges {
        (
            regenerate(
                user.current_charges,
                user.max_charges,
                user.charges_cooldown_ms,
                user.charges_last_updated_at,
                now,
            ) + 30.0 * amount,
            now,
        )
    } else {
        (user.current_charges, user.charges_last_updated_at)
    };

    sqlx::query(
        "UPDATE users SET droplets = $2, max_charges = $3, current_charges = $4, \
         charges_last_updated_at = $5, extra_colors_bitmap = $6, flags_bitmap = $7 \
         WHERE id = $1",
    )
    .bind(auth.id)
    .bind(new_droplets)
    .bind(max_charges)
    .bind(current_charges)
    .bind(charges_last_updated_at)
    .bind(extra_colors)
    .bind(flags_bitmap)
    .execute(&state.pool)
    .await?;
    state.users.refresh(&state.pool, auth.id).await;

    Ok(Json(json!({ "success": true })))
}

// ---------------------------------------------------------------------------
// POST /flag/equip/{id}
// ---------------------------------------------------------------------------

async fn flag_equip(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<Json<Value>> {
    // JS: parseInt + NaN/range check → 400 "Bad Request" (JSON body).
    let Ok(flag_id) = id.parse::<i32>() else {
        return Err(ApiError::bad_request("Bad Request"));
    };
    if !(0..=251).contains(&flag_id) {
        return Err(ApiError::bad_request("Bad Request"));
    }

    let user = state
        .users
        .get_or_load(&state.pool, auth.id)
        .await
        .ok_or(ApiError::Unauthorized)?;

    // Unequip (flagId = 0): only allowed when a flag is currently equipped.
    if flag_id == 0 {
        if user.equipped_flag > 0 {
            sqlx::query("UPDATE users SET equipped_flag = 0 WHERE id = $1")
                .bind(auth.id)
                .execute(&state.pool)
                .await?;
            state.users.refresh(&state.pool, auth.id).await;
            return Ok(Json(json!({ "success": true })));
        }
        return Err(ApiError::forbidden("Forbidden"));
    }

    // Equip: the flag bit must be purchased.
    let flags = WplaceBitMap::from_bytes(user.flags_bitmap.clone());
    if !flags.get(flag_id as u32) {
        return Err(ApiError::forbidden("Forbidden"));
    }
    sqlx::query("UPDATE users SET equipped_flag = $2 WHERE id = $1")
        .bind(auth.id)
        .bind(flag_id)
        .execute(&state.pool)
        .await?;
    state.users.refresh(&state.pool, auth.id).await;
    Ok(Json(json!({ "success": true })))
}
