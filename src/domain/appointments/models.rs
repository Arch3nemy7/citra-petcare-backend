use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, sqlx::Type, ToSchema,
)]
#[sqlx(type_name = "appointment_status", rename_all = "SCREAMING_SNAKE_CASE")]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AppointmentStatus {
    #[default]
    Scheduled,
    Done,
    Cancelled,
    NoShow,
}

/// A booked slot in the clinic agenda. Patient and owner names are
/// denormalized for the agenda view.
#[derive(Debug, Clone)]
pub struct Appointment {
    pub id: Uuid,
    pub patient_id: Uuid,
    pub patient_name: String,
    pub owner_name: String,
    pub scheduled_at: DateTime<Utc>,
    pub reason: String,
    pub status: AppointmentStatus,
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
