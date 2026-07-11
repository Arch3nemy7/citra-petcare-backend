use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type, ToSchema)]
#[sqlx(type_name = "species", rename_all = "SCREAMING_SNAKE_CASE")]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Species {
    Cat,
    Dog,
    Rabbit,
    Bird,
    Hamster,
    Reptile,
    Other,
}

#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, sqlx::Type, ToSchema,
)]
#[sqlx(type_name = "sex", rename_all = "SCREAMING_SNAKE_CASE")]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Sex {
    Male,
    Female,
    #[default]
    Unknown,
}

#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, sqlx::Type, ToSchema,
)]
#[sqlx(type_name = "patient_status", rename_all = "SCREAMING_SNAKE_CASE")]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PatientStatus {
    #[default]
    Active,
    Deceased,
    Inactive,
}

/// An animal patient. `owner_name` is denormalized from the owners table on
/// every read — each pet is shown with its owner throughout the app. Both are
/// None for detached pets ("Tanpa pemilik"): registration without owner data,
/// or the owner record was deleted.
#[derive(Debug, Clone)]
pub struct Patient {
    pub id: Uuid,
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
