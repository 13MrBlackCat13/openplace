// Report endpoints — port of src/routes/report-user.ts:
//   POST /report-user               (auth)                — create a ticket
//   POST /admin/ban-user            (auth + admin!)       — create + resolve ban
//   POST /moderator/timeout-user    (auth + admin!)       — create + resolve timeout
// multer (memoryStorage, files: 1, image/* filter) → axum Multipart.
use std::collections::HashMap;

use axum::extract::{Multipart, State};
use axum::http::header;
use axum::routing::post;
use axum::{Json, Router};
use serde_json::{json, Value};

use crate::auth::{require_admin, AuthUser};
use crate::error::{ApiError, ApiResult};
use crate::services::ticket;
use crate::state::AppState;

/// multer `limits.fileSize` (10MB).
const MAX_IMAGE_BYTES: usize = 10 * 1024 * 1024;
/// BanReason values from src/types/index.ts.
const BAN_REASONS: [&str; 8] = [
    "inappropriate-content",
    "hate-speech",
    "doxxing",
    "bot",
    "griefing",
    "multi-accounting",
    "other",
    "ip-list",
];

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/report-user", post(report_user))
        .route("/admin/ban-user", post(admin_ban_user))
        .route("/moderator/timeout-user", post(admin_timeout_user))
}

// ---------------------------------------------------------------------------
// Multipart form handling
// ---------------------------------------------------------------------------

struct ReportForm {
    fields: HashMap<String, String>,
    /// (content type, bytes) of the first `image` part (multer files: 1).
    image: Option<(String, Vec<u8>)>,
}

fn multipart_error(err: axum::extract::multipart::MultipartError) -> ApiError {
    eprintln!("[multipart] {err}");
    ApiError::bad_request("Invalid multipart form data")
}

async fn read_multipart(mut multipart: Multipart) -> ApiResult<ReportForm> {
    let mut fields: HashMap<String, String> = HashMap::new();
    let mut image: Option<(String, Vec<u8>)> = None;

    while let Some(mut field) = multipart.next_field().await.map_err(multipart_error)? {
        let name = field.name().unwrap_or_default().to_string();
        if name == "image" {
            if image.is_some() {
                continue; // files: 1 — only the first upload is kept
            }
            let content_type = field
                .headers()
                .get(header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .unwrap_or_default()
                .to_string();
            let mut bytes = Vec::new();
            while let Some(chunk) = field.chunk().await.map_err(multipart_error)? {
                if bytes.len() + chunk.len() > MAX_IMAGE_BYTES {
                    return Err(ApiError::bad_request("Image file too large (max 10MB)"));
                }
                bytes.extend_from_slice(&chunk);
            }
            image = Some((content_type, bytes));
        } else {
            let mut buf = Vec::new();
            while let Some(chunk) = field.chunk().await.map_err(multipart_error)? {
                if buf.len() > 1 << 20 {
                    return Err(ApiError::bad_request("Invalid multipart form data"));
                }
                buf.extend_from_slice(&chunk);
            }
            fields.insert(name, String::from_utf8_lossy(&buf).into_owned());
        }
    }

    Ok(ReportForm { fields, image })
}

// ---------------------------------------------------------------------------
// JS number parsing (Number.parseInt / Number.parseFloat semantics)
// ---------------------------------------------------------------------------

/// Number.parseInt: leading whitespace, optional sign, leading digits —
/// anything else is NaN (None here).
pub(crate) fn js_parse_int(s: &str) -> Option<f64> {
    let t = s.trim_start();
    let bytes = t.as_bytes();
    let mut end = 0usize;
    if end < bytes.len() && (bytes[end] == b'+' || bytes[end] == b'-') {
        end += 1;
    }
    let digits_start = end;
    while end < bytes.len() && bytes[end].is_ascii_digit() {
        end += 1;
    }
    if end == digits_start {
        return None; // NaN
    }
    t[..end].parse::<f64>().ok()
}

/// Number.parseFloat: longest valid float prefix — anything else is NaN.
pub(crate) fn js_parse_float(s: &str) -> Option<f64> {
    let t = s.trim_start();
    let bytes = t.as_bytes();
    let mut end = 0usize;
    if end < bytes.len() && (bytes[end] == b'+' || bytes[end] == b'-') {
        end += 1;
    }
    let mut seen_digit = false;
    while end < bytes.len() && bytes[end].is_ascii_digit() {
        end += 1;
        seen_digit = true;
    }
    if end < bytes.len() && bytes[end] == b'.' {
        let dot = end;
        end += 1;
        let mut frac_digit = false;
        while end < bytes.len() && bytes[end].is_ascii_digit() {
            end += 1;
            frac_digit = true;
        }
        if !frac_digit {
            end = dot; // a lone "." is not part of the number
        }
        seen_digit |= frac_digit;
    }
    if !seen_digit {
        return None;
    }
    if end < bytes.len() && (bytes[end] == b'e' || bytes[end] == b'E') {
        let mut exp_end = end + 1;
        if exp_end < bytes.len() && (bytes[exp_end] == b'+' || bytes[exp_end] == b'-') {
            exp_end += 1;
        }
        let digits_start = exp_end;
        while exp_end < bytes.len() && bytes[exp_end].is_ascii_digit() {
            exp_end += 1;
        }
        if exp_end > digits_start {
            end = exp_end;
        }
    }
    t[..end].parse::<f64>().ok()
}

/// JS truthiness for parsed numbers: NaN and 0 are falsy.
fn js_truthy(v: Option<f64>) -> bool {
    matches!(v, Some(x) if x != 0.0)
}

// ---------------------------------------------------------------------------
// makeTicket — same validation order as src/routes/report-user.ts
// ---------------------------------------------------------------------------

async fn make_ticket(state: &AppState, auth: &AuthUser, form: ReportForm) -> ApiResult<uuid::Uuid> {
    let f = &form.fields;
    // JS: `req.body.X ?? -1` — absent fields fall back to -1 (truthy), so only
    // present-but-falsy values (""/"0"/garbage) hit "Parameters missing" and a
    // missing reportedUserId is caught by the negative-integer check.
    let reported = js_parse_int(f.get("reportedUserId").map(String::as_str).unwrap_or("-1"));
    let latitude = js_parse_float(f.get("latitude").map(String::as_str).unwrap_or("-1"));
    let longitude = js_parse_float(f.get("longitude").map(String::as_str).unwrap_or("-1"));
    let zoom = js_parse_float(f.get("zoom").map(String::as_str).unwrap_or("-1"));
    let reason = f.get("reason").map(String::as_str).unwrap_or("");
    let notes = f.get("notes").map(String::as_str).unwrap_or("");

    if !js_truthy(reported)
        || !js_truthy(latitude)
        || !js_truthy(longitude)
        || !js_truthy(zoom)
        || reason.is_empty()
    {
        return Err(ApiError::bad_request("Parameters missing"));
    }

    // Negative user ids correlate to the suspended/system accounts — blocked.
    let reported_id = reported.unwrap() as i64;
    if reported_id < 0 {
        return Err(ApiError::bad_request(
            "You cannot report a user id with a negative integer.",
        ));
    }

    if notes.len() < 5 {
        return Err(ApiError::bad_request("Note must be at least 5 characters"));
    }

    let Some((content_type, image_bytes)) = form.image else {
        return Err(ApiError::bad_request("Image is required"));
    };

    // multer fileFilter: mimetype must start with "image/".
    if !content_type.starts_with("image/") {
        return Err(ApiError::bad_request("Only image files are allowed"));
    }

    if latitude.unwrap().is_nan() || longitude.unwrap().is_nan() || zoom.unwrap().is_nan() {
        return Err(ApiError::bad_request("Invalid coordinates"));
    }

    if !BAN_REASONS.contains(&reason) {
        return Err(ApiError::bad_request("Invalid ban reason"));
    }

    let Ok(reported_id) = i32::try_from(reported_id) else {
        // JS would pass an out-of-range id straight into prisma → 500.
        return Err(ApiError::Internal);
    };

    // TicketService.reportUser draws the upload onto a canvas and stores
    // image/jpeg — re-encode instead of storing the raw upload.
    let img = image::load_from_memory(&image_bytes)
        .map_err(|_| ApiError::bad_request("Invalid image file content"))?;
    let mut jpeg = Vec::new();
    img.write_with_encoder(image::codecs::jpeg::JpegEncoder::new_with_quality(
        &mut jpeg, 80,
    ))
    .map_err(|_| ApiError::Internal)?;

    ticket::create_ticket(
        &state.pool,
        auth.id,
        reported_id,
        latitude.unwrap(),
        longitude.unwrap(),
        zoom.unwrap(),
        reason,
        notes,
        Some(jpeg),
    )
    .await
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// POST /report-user (auth)
async fn report_user(
    State(state): State<AppState>,
    auth: AuthUser,
    multipart: Multipart,
) -> ApiResult<Json<Value>> {
    let form = read_multipart(multipart).await?;
    make_ticket(&state, &auth, form).await?;
    Ok(Json(json!({})))
}

/// POST /admin/ban-user (auth + adminMiddleware) — create ticket + ban resolve.
async fn admin_ban_user(
    State(state): State<AppState>,
    auth: AuthUser,
    multipart: Multipart,
) -> ApiResult<Json<Value>> {
    require_admin(&state, auth.id).await?;
    let form = read_multipart(multipart).await?;
    let ticket_id = make_ticket(&state, &auth, form).await?;
    ticket::resolve(&state, ticket_id, auth.id, "ban", None).await?;
    Ok(Json(json!({})))
}

/// POST /moderator/timeout-user — declared in admin.ts with adminMiddleware
/// (admin, not moderator!), create ticket + timeout resolve.
async fn admin_timeout_user(
    State(state): State<AppState>,
    auth: AuthUser,
    multipart: Multipart,
) -> ApiResult<Json<Value>> {
    require_admin(&state, auth.id).await?;
    let form = read_multipart(multipart).await?;
    let ticket_id = make_ticket(&state, &auth, form).await?;
    ticket::resolve(&state, ticket_id, auth.id, "timeout", None).await?;
    Ok(Json(json!({})))
}
