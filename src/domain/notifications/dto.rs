use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use super::models::Notification;

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct NotificationResponse {
    pub id: Uuid,
    #[serde(rename = "type")]
    #[schema(example = "VACCINATION_DUE")]
    pub notif_type: String,
    pub title: String,
    pub body: String,
    /// Structured details (e.g. the list of due vaccinations).
    pub payload: serde_json::Value,
    pub sent_at: Option<DateTime<Utc>>,
    pub read_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<Notification> for NotificationResponse {
    fn from(n: Notification) -> Self {
        Self {
            id: n.id,
            notif_type: n.notif_type,
            title: n.title,
            body: n.body,
            payload: n.payload,
            sent_at: n.sent_at,
            read_at: n.read_at,
            created_at: n.created_at,
            updated_at: n.updated_at,
        }
    }
}

#[derive(Debug, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct ListNotificationsParams {
    /// When true, only notifications that have not been marked read.
    pub unread: Option<bool>,
    pub cursor: Option<Uuid>,
    pub limit: Option<i64>,
}
