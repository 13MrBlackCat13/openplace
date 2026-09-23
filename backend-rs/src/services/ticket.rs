use sqlx::PgPool;
use uuid::Uuid;

use crate::error::ApiResult;
use crate::services::notification::NotificationService;
use crate::services::user_ops;
use crate::state::AppState;

/// CONTRACT (do not change signatures — routes depend on them).
pub async fn create_ticket(
    pool: &PgPool,
    user_id: i32,
    reported_user_id: i32,
    latitude: f64,
    longitude: f64,
    zoom: f64,
    reason: &str,
    notes: &str,
    image: Option<Vec<u8>>,
) -> ApiResult<Uuid> {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO tickets (id, user_id, reported_user_id, latitude, longitude, zoom, reason, notes, image) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
    )
    .bind(id)
    .bind(user_id)
    .bind(reported_user_id)
    .bind(latitude)
    .bind(longitude)
    .bind(zoom)
    .bind(reason)
    .bind(notes)
    .bind(image)
    .execute(pool)
    .await?;
    Ok(id)
}

/// Resolve a ticket: ignore / timeout (3 days) / ban (+IP cascade).
/// Notifies the reporter for non-ignore resolutions (JS parity).
pub async fn resolve(
    state: &AppState,
    ticket_id: Uuid,
    moderator_user_id: i32,
    resolution: &str,
    assigned_reason: Option<String>,
) -> ApiResult<()> {
    // moderator.ts writes an assigned reason back onto the ticket first, but
    // only for bans — folded into the UPDATE ... RETURNING here.
    let reason_override = match assigned_reason {
        Some(reason) if resolution == "ban" && !reason.is_empty() => Some(reason),
        _ => None,
    };

    let resolved: Option<(Option<i32>, Option<i32>, String)> = if let Some(reason) = reason_override
    {
        sqlx::query_as(
            "UPDATE tickets SET resolution = $2, moderator_user_id = $3, reason = $4, updated_at = now() \
             WHERE id = $1 \
             RETURNING reported_user_id, user_id, reason",
        )
        .bind(ticket_id)
        .bind(resolution)
        .bind(moderator_user_id)
        .bind(reason)
        .fetch_optional(&state.pool)
        .await?
    } else {
        sqlx::query_as(
            "UPDATE tickets SET resolution = $2, moderator_user_id = $3, updated_at = now() \
             WHERE id = $1 \
             RETURNING reported_user_id, user_id, reason",
        )
        .bind(ticket_id)
        .bind(resolution)
        .bind(moderator_user_id)
        .fetch_optional(&state.pool)
        .await?
    };

    // Unknown ticket id: nothing to resolve (JS would 500 on the missing
    // prisma row; valid ids behave identically).
    let Some((reported_user_id, reporter_id, reason)) = resolved else {
        return Ok(());
    };

    match resolution {
        "timeout" => {
            if let Some(reported) = reported_user_id {
                user_ops::timeout_user(state, reported, 3).await?;
            }
        }
        "ban" => {
            if let Some(reported) = reported_user_id {
                // ban_user cascades to accounts sharing the banned user's IPs.
                user_ops::ban_user(state, reported, true, Some(reason)).await?;
            }
        }
        _ => {} // "ignore"
    }

    if resolution != "ignore" {
        if let Some(reporter) = reporter_id {
            if reporter != moderator_user_id {
                NotificationService::create(
                    &state.pool,
                    reporter,
                    "report",
                    "Update on your report",
                    "Thank you for reporting a violation of the rules. The moderators have reviewed your report and taken appropriate action.",
                )
                .await?;
            }
        }
    }
    Ok(())
}
