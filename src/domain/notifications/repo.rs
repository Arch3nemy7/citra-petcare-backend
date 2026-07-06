use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use super::models::Notification;
use crate::error::AppError;

pub async fn insert(
    db: &PgPool,
    id: Uuid,
    notif_type: &str,
    title: &str,
    body: &str,
    payload: &serde_json::Value,
) -> Result<Notification, AppError> {
    let notification = sqlx::query_as!(
        Notification,
        r#"
        INSERT INTO notifications (id, type, title, body, payload)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING id, type AS notif_type, title, body, payload, sent_at, read_at,
                  created_at, updated_at
        "#,
        id,
        notif_type,
        title,
        body,
        payload
    )
    .fetch_one(db)
    .await?;
    Ok(notification)
}

pub async fn list(
    db: &PgPool,
    unread_only: bool,
    cursor: Option<Uuid>,
    limit: i64,
) -> Result<Vec<Notification>, AppError> {
    let rows = sqlx::query_as!(
        Notification,
        r#"
        SELECT id, type AS notif_type, title, body, payload, sent_at, read_at,
               created_at, updated_at
        FROM notifications
        WHERE deleted_at IS NULL
          AND (NOT $1 OR read_at IS NULL)
          AND ($2::uuid IS NULL OR id < $2)
        ORDER BY id DESC
        LIMIT $3
        "#,
        unread_only,
        cursor,
        limit + 1
    )
    .fetch_all(db)
    .await?;
    Ok(rows)
}

pub async fn mark_read(db: &PgPool, id: Uuid) -> Result<bool, AppError> {
    let result = sqlx::query!(
        "UPDATE notifications SET read_at = now() WHERE id = $1 AND read_at IS NULL AND deleted_at IS NULL",
        id
    )
    .execute(db)
    .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn mark_sent(db: &PgPool, id: Uuid) -> Result<(), AppError> {
    sqlx::query!("UPDATE notifications SET sent_at = now() WHERE id = $1", id)
        .execute(db)
        .await?;
    Ok(())
}

/// Dedup helper for the scheduler: has a notification of this type already
/// been created since `since` (start of the Jakarta day)?
pub async fn exists_since(
    db: &PgPool,
    notif_type: &str,
    since: DateTime<Utc>,
) -> Result<bool, AppError> {
    let exists = sqlx::query_scalar!(
        r#"
        SELECT EXISTS(
            SELECT 1 FROM notifications
            WHERE type = $1 AND created_at >= $2 AND deleted_at IS NULL
        ) AS "exists!"
        "#,
        notif_type,
        since
    )
    .fetch_one(db)
    .await?;
    Ok(exists)
}
