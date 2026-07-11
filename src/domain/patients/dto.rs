use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;
use validator::Validate;

use super::models::{Patient, PatientStatus, Sex, Species};

/// Body for both POST (create) and PUT (idempotent upsert).
#[derive(Debug, Deserialize, Validate, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PatientRequest {
    /// Client-generated UUIDv7. Optional on POST; ignored on PUT (path wins).
    pub id: Option<Uuid>,
    /// Omit for a pet without owner data ("Tanpa pemilik").
    pub owner_id: Option<Uuid>,
    #[validate(length(min = 1, max = 100))]
    pub name: String,
    pub species: Species,
    #[validate(length(max = 100))]
    pub breed: Option<String>,
    #[serde(default)]
    pub sex: Sex,
    #[serde(default)]
    pub sterilized: bool,
    pub birth_date: Option<NaiveDate>,
    #[validate(length(max = 200))]
    pub color_markings: Option<String>,
    #[validate(length(max = 50))]
    pub microchip_no: Option<String>,
    /// Storage key of the pet photo (from /storage/presign-upload).
    #[validate(length(max = 500))]
    pub photo_key: Option<String>,
    #[validate(length(max = 1000))]
    pub allergies: Option<String>,
    /// Safety notes surfaced prominently, e.g. "aggressive, needs muzzle".
    #[validate(length(max = 1000))]
    pub alert_notes: Option<String>,
    #[serde(default)]
    pub status: PatientStatus,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PatientResponse {
    pub id: Uuid,
    /// None for detached pets ("Tanpa pemilik").
    pub owner_id: Option<Uuid>,
    pub owner_name: Option<String>,
    pub name: String,
    pub species: Species,
    pub breed: Option<String>,
    pub sex: Sex,
    pub sterilized: bool,
    pub birth_date: Option<NaiveDate>,
    pub color_markings: Option<String>,
    pub microchip_no: Option<String>,
    pub photo_key: Option<String>,
    pub allergies: Option<String>,
    pub alert_notes: Option<String>,
    pub status: PatientStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<Patient> for PatientResponse {
    fn from(p: Patient) -> Self {
        Self {
            id: p.id,
            owner_id: p.owner_id,
            owner_name: p.owner_name,
            name: p.name,
            species: p.species,
            breed: p.breed,
            sex: p.sex,
            sterilized: p.sterilized,
            birth_date: p.birth_date,
            color_markings: p.color_markings,
            microchip_no: p.microchip_no,
            photo_key: p.photo_key,
            allergies: p.allergies,
            alert_notes: p.alert_notes,
            status: p.status,
            created_at: p.created_at,
            updated_at: p.updated_at,
        }
    }
}

#[derive(Debug, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct ListPatientsParams {
    /// Case-insensitive substring match on patient name, owner name or phone.
    pub search: Option<String>,
    /// Restrict to one owner's pets.
    pub owner_id: Option<Uuid>,
    pub cursor: Option<Uuid>,
    pub limit: Option<i64>,
}

/// One weight measurement, taken from a visit record.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WeightPointResponse {
    pub visit_id: Uuid,
    pub visit_date: DateTime<Utc>,
    pub weight_kg: f64,
}
