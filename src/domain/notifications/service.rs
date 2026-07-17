use std::collections::BTreeMap;

use sqlx::PgPool;
use uuid::Uuid;

use super::models::Notification;
use super::{Notifier, OutboundMessage, repo};
use crate::error::AppError;
use crate::http::pagination::Paginated;

pub async fn list(
    db: &PgPool,
    unread_only: bool,
    cursor: Option<Uuid>,
    limit: i64,
) -> Result<Paginated<Notification>, AppError> {
    let rows = repo::list(db, unread_only, cursor, limit).await?;
    Ok(Paginated::from_rows(rows, limit, |n| n.id))
}

pub async fn mark_read(db: &PgPool, id: Uuid) -> Result<(), AppError> {
    if !repo::mark_read(db, id).await? {
        return Err(AppError::NotFound("notification"));
    }
    Ok(())
}

/// Persist a notification row, then push it through the Notifier. The row is
/// the source of truth; a failed push is logged and leaves `sent_at` NULL so
/// it is visible in the app regardless.
pub async fn create_and_dispatch(
    db: &PgPool,
    notifier: &dyn Notifier,
    notif_type: &str,
    title: String,
    body: String,
    payload: serde_json::Value,
) -> Result<Notification, AppError> {
    let notification =
        repo::insert(db, Uuid::now_v7(), notif_type, &title, &body, &payload).await?;

    let mut data = BTreeMap::new();
    data.insert("type".to_string(), notif_type.to_string());
    data.insert("notificationId".to_string(), notification.id.to_string());
    let message = OutboundMessage { title, body, data };

    match notifier.send(&message).await {
        Ok(()) => {
            // Best-effort, matching this function's doctrine that the row is the
            // source of truth: a failed `mark_sent` must not propagate and abort
            // the caller (the daily scheduler dispatches several categories in
            // sequence — one transient DB hiccup here should not drop the rest).
            if let Err(error) = repo::mark_sent(db, notification.id).await {
                tracing::warn!(id = %notification.id, %error, "failed to record sent_at after successful push");
            } else {
                tracing::info!(id = %notification.id, driver = notifier.name(), r#type = notif_type, "notification sent");
            }
        }
        Err(error) => {
            tracing::warn!(id = %notification.id, driver = notifier.name(), %error, "notification push failed; row persisted");
        }
    }
    Ok(notification)
}
