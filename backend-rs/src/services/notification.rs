use axum::Json;
use serde_json::json;
use sqlx::PgPool;

use crate::error::ApiResult;

/// CONTRACT (do not change signatures — routes depend on them).
pub struct NotificationService;

impl NotificationService {
    pub async fn create(
        pool: &PgPool,
        user_id: i32,
        icon: &str,
        title: &str,
        message: &str,
    ) -> ApiResult<()> {
        sqlx::query("INSERT INTO notifications (user_id, sending_user_id, icon, title, message) VALUES ($1, -1, $2, $3, $4)")
            .bind(user_id)
            .bind(icon)
            .bind(title)
            .bind(message)
            .execute(pool)
            .await?;
        Ok(())
    }

    pub async fn create_from_user(
        pool: &PgPool,
        user_id: i32,
        sending_user_id: i32,
        icon: &str,
        title: &str,
        message: &str,
    ) -> ApiResult<()> {
        sqlx::query("INSERT INTO notifications (user_id, sending_user_id, icon, title, message) VALUES ($1, $2, $3, $4, $5)")
            .bind(user_id)
            .bind(sending_user_id)
            .bind(icon)
            .bind(title)
            .bind(message)
            .execute(pool)
            .await?;
        Ok(())
    }

    pub async fn create_system(
        pool: &PgPool,
        icon: &str,
        title: &str,
        message: &str,
    ) -> ApiResult<()> {
        sqlx::query("INSERT INTO system_notifications (icon, title, message) VALUES ($1, $2, $3)")
            .bind(icon)
            .bind(title)
            .bind(message)
            .execute(pool)
            .await?;
        Ok(())
    }

    pub async fn unread_count(pool: &PgPool, user_id: i32) -> ApiResult<i64> {
        let personal: (i64,) = sqlx::query_as(
            "SELECT count(*) FROM notifications WHERE user_id = $1 AND read = false",
        )
        .bind(user_id)
        .fetch_one(pool)
        .await?;
        let system: (i64,) = sqlx::query_as(
            "SELECT count(*) FROM system_notifications s \
             WHERE NOT EXISTS (SELECT 1 FROM system_notification_reads r \
                               WHERE r.system_notification_id = s.id AND r.user_id = $1)",
        )
        .bind(user_id)
        .fetch_one(pool)
        .await?;
        Ok(personal.0 + system.0)
    }

    /// Notification page: personal + system (negative ids), newest first.
    pub async fn page(
        pool: &PgPool,
        user_id: i32,
        limit: i64,
    ) -> ApiResult<Json<serde_json::Value>> {
        #[derive(sqlx::FromRow)]
        struct Row {
            id: i32,
            #[sqlx(rename = "read")]
            is_read: bool,
            created_at: chrono::DateTime<chrono::Utc>,
            icon: String,
            title: String,
            message: String,
            sender_name: Option<String>,
            is_system: bool,
        }
        let rows: Vec<Row> = sqlx::query_as(
            r#"
            SELECT n.id, n.read, n.created_at, n.icon, n.title, n.message,
                   COALESCE(u.nickname, u.name) AS sender_name, false AS is_system
            FROM notifications n LEFT JOIN users u ON u.id = n.sending_user_id
            WHERE n.user_id = $1
            UNION ALL
            SELECT -s.id, (EXISTS (SELECT 1 FROM system_notification_reads r
                        WHERE r.system_notification_id = s.id AND r.user_id = $1)) AS read,
                   s.created_at, s.icon, s.title, s.message, 'System' AS sender_name, true AS is_system
            FROM system_notifications s
            ORDER BY created_at DESC
            LIMIT $2
            "#,
        )
        .bind(user_id)
        .bind(limit)
        .fetch_all(pool)
        .await?;
        let notifications: Vec<serde_json::Value> = rows
            .into_iter()
            .map(|r| {
                json!({
                    "id": r.id,
                    "read": r.is_read,
                    "createdAt": r.created_at.to_rfc3339(),
                    "type": "report_feedback",
                    "icon": r.icon,
                    "title": r.title,
                    "message": crate::services::notification::render_markdown(&r.message),
                    "sendingUser": {
                        "id": if r.is_system { -1 } else { 0 },
                        "name": r.sender_name.unwrap_or_default(),
                    },
                    "isSystem": r.is_system,
                })
            })
            .collect();
        Ok(Json(json!({ "notifications": notifications })))
    }

    /// ids > 0 → personal notifications; ids < 0 → system (abs = -id).
    pub async fn mark_read(pool: &PgPool, user_id: i32, ids: &[i64]) -> ApiResult<()> {
        let personal: Vec<i64> = ids.iter().filter(|id| **id > 0).copied().collect();
        let system: Vec<i64> = ids.iter().filter(|id| **id < 0).map(|id| -id).collect();
        if !personal.is_empty() {
            sqlx::query("UPDATE notifications SET read = true WHERE user_id = $1 AND id = ANY($2)")
                .bind(user_id)
                .bind(&personal)
                .execute(pool)
                .await?;
        }
        if !system.is_empty() {
            sqlx::query(
                "INSERT INTO system_notification_reads (system_notification_id, user_id) \
                 SELECT id, $2 FROM system_notifications WHERE id = ANY($1) \
                 ON CONFLICT (system_notification_id, user_id) DO NOTHING",
            )
            .bind(&system)
            .bind(user_id)
            .execute(pool)
            .await?;
        }
        Ok(())
    }

    pub async fn mark_all_read(pool: &PgPool, user_id: i32) -> ApiResult<()> {
        sqlx::query("UPDATE notifications SET read = true WHERE user_id = $1 AND read = false")
            .bind(user_id)
            .execute(pool)
            .await?;
        sqlx::query(
            "INSERT INTO system_notification_reads (system_notification_id, user_id) \
             SELECT id, $1 FROM system_notifications \
             ON CONFLICT (system_notification_id, user_id) DO NOTHING",
        )
        .bind(user_id)
        .execute(pool)
        .await?;
        Ok(())
    }
}

/// markdown-it equivalent for notification bodies.
pub fn render_markdown(source: &str) -> String {
    use pulldown_cmark::{html, Parser};
    let parser = Parser::new(source);
    let mut out = String::with_capacity(source.len());
    html::push_html(&mut out, parser);
    out
}
