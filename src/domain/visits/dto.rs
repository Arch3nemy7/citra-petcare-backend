use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;
use validator::Validate;

use super::models::{AttachmentKind, Visit, VisitAttachment, VisitStockUsage, VisitType};

/// Body for both POST (create) and PUT (idempotent upsert).
#[derive(Debug, Deserialize, Validate, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct VisitRequest {
    /// Client-generated UUIDv7. Optional on POST; ignored on PUT (path wins).
    pub id: Option<Uuid>,
    pub patient_id: Uuid,
    /// The attending vet (a user id).
    pub vet_id: Uuid,
    /// Defaults to PERIKSA when omitted (kept optional for older clients).
    #[serde(default)]
    pub visit_type: VisitType,
    pub visit_date: DateTime<Utc>,
    /// Anamnesis — the owner's account of the problem.
    #[validate(length(min = 1, max = 4000))]
    pub complaint: String,
    #[validate(range(min = 25.0, max = 46.0))]
    pub temperature_c: Option<f64>,
    #[validate(range(min = 0.001, max = 500.0))]
    pub weight_kg: Option<f64>,
    #[validate(length(max = 8000))]
    pub exam_notes: Option<String>,
    #[validate(length(max = 4000))]
    pub diagnosis: Option<String>,
    #[validate(length(max = 4000))]
    pub treatment: Option<String>,
    #[validate(length(max = 4000))]
    pub prescription: Option<String>,
    pub follow_up_date: Option<NaiveDate>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct VisitResponse {
    pub id: Uuid,
    pub patient_id: Uuid,
    pub patient_name: String,
    pub vet_id: Uuid,
    pub vet_name: String,
    pub visit_type: VisitType,
    pub visit_date: DateTime<Utc>,
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

impl From<Visit> for VisitResponse {
    fn from(v: Visit) -> Self {
        Self {
            id: v.id,
            patient_id: v.patient_id,
            patient_name: v.patient_name,
            vet_id: v.vet_id,
            vet_name: v.vet_name,
            visit_type: v.visit_type,
            visit_date: v.visit_date,
            complaint: v.complaint,
            temperature_c: v.temperature_c,
            weight_kg: v.weight_kg,
            exam_notes: v.exam_notes,
            diagnosis: v.diagnosis,
            treatment: v.treatment,
            prescription: v.prescription,
            follow_up_date: v.follow_up_date,
            created_at: v.created_at,
            updated_at: v.updated_at,
        }
    }
}

/// A visit plus its attachments and the stock it consumed (returned by
/// GET /visits/{id}).
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct VisitDetailResponse {
    #[serde(flatten)]
    pub visit: VisitResponse,
    pub attachments: Vec<VisitAttachmentResponse>,
    /// Drugs/vaccines deducted from inventory during this visit (OUT
    /// movements linked via visitId).
    pub stock_usage: Vec<VisitStockUsageResponse>,
}

/// One inventory deduction made during the visit.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct VisitStockUsageResponse {
    pub item_id: Uuid,
    pub item_name: String,
    pub unit: String,
    pub qty: f64,
}

impl From<VisitStockUsage> for VisitStockUsageResponse {
    fn from(u: VisitStockUsage) -> Self {
        Self {
            item_id: u.item_id,
            item_name: u.item_name,
            unit: u.unit,
            qty: u.qty,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct VisitAttachmentResponse {
    pub id: Uuid,
    pub visit_id: Uuid,
    pub file_key: String,
    pub kind: AttachmentKind,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<VisitAttachment> for VisitAttachmentResponse {
    fn from(a: VisitAttachment) -> Self {
        Self {
            id: a.id,
            visit_id: a.visit_id,
            file_key: a.file_key,
            kind: a.kind,
            created_at: a.created_at,
            updated_at: a.updated_at,
        }
    }
}

#[derive(Debug, Deserialize, Validate, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentRequest {
    /// Client-generated UUIDv7 (optional).
    pub id: Option<Uuid>,
    /// Storage key returned by /storage/presign-upload.
    #[validate(length(min = 1, max = 500))]
    pub file_key: String,
    #[serde(default)]
    pub kind: AttachmentKind,
}

#[derive(Debug, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct ListVisitsParams {
    pub patient_id: Option<Uuid>,
    /// Only visits at/after this instant (RFC 3339).
    pub from: Option<DateTime<Utc>>,
    /// Only visits before this instant (RFC 3339).
    pub to: Option<DateTime<Utc>>,
    pub cursor: Option<Uuid>,
    pub limit: Option<i64>,
}
