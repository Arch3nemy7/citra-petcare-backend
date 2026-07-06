use chrono::{DateTime, Utc};
use uuid::Uuid;

/// A persisted notification (also pushed via the Notifier when created by the
/// scheduler). `sent_at` stays NULL if the push failed — the row is the
/// source of truth, the push is best-effort.
#[derive(Debug, Clone)]
pub struct Notification {
    pub id: Uuid,
    /// VACCINATION_DUE | LOW_STOCK | EXPIRY_WARNING | ...
    pub notif_type: String,
    pub title: String,
    pub body: String,
    pub payload: serde_json::Value,
    pub sent_at: Option<DateTime<Utc>>,
    pub read_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
