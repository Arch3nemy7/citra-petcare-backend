use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;
use validator::{Validate, ValidationError};

use super::models::Vaccination;

/// Body for POST /patients/{id}/vaccinations — the patient comes from the path.
#[derive(Debug, Deserialize, Validate, ToSchema)]
#[serde(rename_all = "camelCase")]
#[validate(schema(function = "validate_create_dates"))]
pub struct CreateVaccinationRequest {
    /// Client-generated UUIDv7 (optional).
    pub id: Option<Uuid>,
    pub visit_id: Option<Uuid>,
    #[validate(length(min = 1, max = 200))]
    #[schema(example = "Rabies (Rabisin)")]
    pub vaccine_name: String,
    /// Omit for a due-only record — a dose known to be due (e.g. a booster
    /// from another clinic's card) without an administration date.
    pub date_given: Option<NaiveDate>,
    #[validate(length(max = 100))]
    pub batch_no: Option<String>,
    pub next_due_date: Option<NaiveDate>,
}

/// Body for PUT /vaccinations/{id} — the full representation, incl. patient.
#[derive(Debug, Deserialize, Validate, ToSchema)]
#[serde(rename_all = "camelCase")]
#[validate(schema(function = "validate_upsert_dates"))]
pub struct UpsertVaccinationRequest {
    pub patient_id: Uuid,
    pub visit_id: Option<Uuid>,
    #[validate(length(min = 1, max = 200))]
    pub vaccine_name: String,
    pub date_given: Option<NaiveDate>,
    #[validate(length(max = 100))]
    pub batch_no: Option<String>,
    pub next_due_date: Option<NaiveDate>,
}

/// A record needs at least one date to mean anything, and a due date must
/// come after the administration date when both are present.
fn check_dates(
    date_given: Option<NaiveDate>,
    next_due_date: Option<NaiveDate>,
) -> Result<(), ValidationError> {
    match (date_given, next_due_date) {
        (None, None) => Err(ValidationError::new("date_given")
            .with_message("dateGiven or nextDueDate is required".into())),
        (Some(given), Some(due)) if due <= given => Err(ValidationError::new("next_due_date")
            .with_message("nextDueDate must be after dateGiven".into())),
        _ => Ok(()),
    }
}

fn validate_create_dates(req: &CreateVaccinationRequest) -> Result<(), ValidationError> {
    check_dates(req.date_given, req.next_due_date)
}

fn validate_upsert_dates(req: &UpsertVaccinationRequest) -> Result<(), ValidationError> {
    check_dates(req.date_given, req.next_due_date)
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct VaccinationResponse {
    pub id: Uuid,
    pub patient_id: Uuid,
    pub patient_name: String,
    pub visit_id: Option<Uuid>,
    pub vaccine_name: String,
    /// None for due-only records.
    pub date_given: Option<NaiveDate>,
    pub batch_no: Option<String>,
    pub next_due_date: Option<NaiveDate>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<Vaccination> for VaccinationResponse {
    fn from(v: Vaccination) -> Self {
        Self {
            id: v.id,
            patient_id: v.patient_id,
            patient_name: v.patient_name,
            visit_id: v.visit_id,
            vaccine_name: v.vaccine_name,
            date_given: v.date_given,
            batch_no: v.batch_no,
            next_due_date: v.next_due_date,
            created_at: v.created_at,
            updated_at: v.updated_at,
        }
    }
}
