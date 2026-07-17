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
    /// The owner's signed letter of approval for a procedure (anesthesia,
    /// surgery, inpatient care). Attached to STERILISASI/OPNAME visits.
    Consent,
    #[default]
    Other,
}

/// What kind of visit was recorded. Grooming visits skip the medical fields
/// in the app; the others follow the full anamnesis → exam → diagnosis
/// flow. Sterilisasi and Opname additionally carry a CONSENT attachment
/// (the owner's signed approval).
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, sqlx::Type, ToSchema,
)]
#[sqlx(type_name = "visit_type", rename_all = "SCREAMING_SNAKE_CASE")]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum VisitType {
    #[default]
    Periksa,
    Grooming,
    Vaksinasi,
    Sterilisasi,
    /// Inpatient care (hospitalization).
    Opname,
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
    pub visit_type: VisitType,
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

/// One inventory deduction made during a visit — an OUT stock movement
/// joined with the item's display fields, for the "obat & vaksin dipakai"
/// section of the visit detail.
#[derive(Debug, Clone)]
pub struct VisitStockUsage {
    pub item_id: Uuid,
    pub item_name: String,
    pub unit: String,
    pub qty: f64,
}
