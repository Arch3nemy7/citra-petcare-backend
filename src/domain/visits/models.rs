use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, sqlx::Type, ToSchema,
)]
#[sqlx(type_name = "attachment_kind", rename_all = "SCREAMING_SNAKE_CASE")]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AttachmentKind {
    Photo,
    Xray,
    Lab,
    #[default]
    Other,
}

/// A consultation/examination record. Patient and vet names are denormalized
/// for display.
#[derive(Debug, Clone)]
pub struct Visit {
    pub id: Uuid,
    pub patient_id: Uuid,
    pub patient_name: String,
    pub vet_id: Uuid,
    pub vet_name: String,
    pub visit_date: DateTime<Utc>,
    /// Anamnesis — the owner's account of the problem.
    pub complaint: String,
    pub temperature_c: Option<f64>,
    pub weight_kg: Option<f64>,
    pub exam_notes: Option<String>,
    pub diagnosis: Option<String>,
    pub treatment: Option<String>,
    pub prescription: Option<String>,
    pub follow_up_date: Option<NaiveDate>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A file (photo, x-ray, lab result) attached to a visit. The blob itself
/// lives in object storage under `file_key`.
#[derive(Debug, Clone)]
pub struct VisitAttachment {
    pub id: Uuid,
    pub visit_id: Uuid,
    pub file_key: String,
    pub kind: AttachmentKind,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// One weight measurement extracted from a visit.
#[derive(Debug, Clone)]
pub struct WeightPoint {
    pub visit_id: Uuid,
    pub visit_date: DateTime<Utc>,
    pub weight_kg: f64,
}
