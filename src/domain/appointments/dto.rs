use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;
use validator::Validate;

use super::models::{Appointment, AppointmentStatus};
use crate::domain::patients::models::Species;

/// Body for both POST (create) and PUT (idempotent upsert).
#[derive(Debug, Deserialize, Validate, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AppointmentRequest {
    /// Client-generated UUIDv7. Optional on POST; ignored on PUT (path wins).
    pub id: Option<Uuid>,
    pub patient_id: Uuid,
    pub scheduled_at: DateTime<Utc>,
    #[validate(length(min = 1, max = 500))]
    #[schema(example = "Vaksinasi tahunan")]
    pub reason: String,
    #[serde(default)]
    pub status: AppointmentStatus,
    #[validate(length(max = 2000))]
    pub notes: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AppointmentResponse {
    pub id: Uuid,
    pub patient_id: Uuid,
    pub patient_name: String,
    /// For the agenda's pet avatar (species-tinted initial fallback).
    pub patient_species: Species,
    pub patient_photo_key: Option<String>,
    pub owner_name: Option<String>,
    pub scheduled_at: DateTime<Utc>,
    pub reason: String,
    pub status: AppointmentStatus,
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<Appointment> for AppointmentResponse {
    fn from(a: Appointment) -> Self {
        Self {
            id: a.id,
            patient_id: a.patient_id,
            patient_name: a.patient_name,
            patient_species: a.patient_species,
            patient_photo_key: a.patient_photo_key,
            owner_name: a.owner_name,
            scheduled_at: a.scheduled_at,
            reason: a.reason,
            status: a.status,
            notes: a.notes,
            created_at: a.created_at,
            updated_at: a.updated_at,
        }
    }
}

#[derive(Debug, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct ListAppointmentsParams {
    /// Only appointments at/after this instant (RFC 3339).
    pub from: Option<DateTime<Utc>>,
    /// Only appointments before this instant (RFC 3339).
    pub to: Option<DateTime<Utc>>,
    pub status: Option<AppointmentStatus>,
    pub patient_id: Option<Uuid>,
    pub cursor: Option<Uuid>,
    pub limit: Option<i64>,
}
