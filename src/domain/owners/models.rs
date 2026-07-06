use chrono::{DateTime, Utc};
use uuid::Uuid;

/// A pet owner (client of the clinic).
#[derive(Debug, Clone)]
pub struct Owner {
    pub id: Uuid,
    pub name: String,
    pub phone: String,
    pub alt_phone: Option<String>,
    pub address: Option<String>,
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
