use chrono::{DateTime, Utc};
use uuid::Uuid;

/// A pet owner (client of the clinic). Phone is optional — walk-ins are
/// sometimes registered before any contact data is known.
#[derive(Debug, Clone)]
pub struct Owner {
    pub id: Uuid,
    pub name: String,
    pub phone: Option<String>,
    pub alt_phone: Option<String>,
    pub address: Option<String>,
    pub notes: Option<String>,
    /// Active (non-deleted) pets registered under this owner.
    pub patient_count: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
