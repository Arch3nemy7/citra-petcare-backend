use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;
use validator::Validate;

use super::models::Owner;

/// Body for both POST (create) and PUT (idempotent upsert).
#[derive(Debug, Deserialize, Validate, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct OwnerRequest {
    /// Client-generated UUIDv7. Optional on POST (server generates one);
    /// ignored on PUT, where the path id wins.
    pub id: Option<Uuid>,
    #[validate(length(min = 1, max = 200))]
    pub name: String,
    #[validate(length(min = 5, max = 32))]
    #[schema(example = "+62812xxxxxx")]
    pub phone: String,
    #[validate(length(min = 5, max = 32))]
    pub alt_phone: Option<String>,
    #[validate(length(max = 500))]
    pub address: Option<String>,
    #[validate(length(max = 2000))]
    pub notes: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct OwnerResponse {
    pub id: Uuid,
    pub name: String,
    pub phone: String,
    pub alt_phone: Option<String>,
    pub address: Option<String>,
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<Owner> for OwnerResponse {
    fn from(owner: Owner) -> Self {
        Self {
            id: owner.id,
            name: owner.name,
            phone: owner.phone,
            alt_phone: owner.alt_phone,
            address: owner.address,
            notes: owner.notes,
            created_at: owner.created_at,
            updated_at: owner.updated_at,
        }
    }
}

#[derive(Debug, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct ListOwnersParams {
    /// Case-insensitive substring match on name or phone.
    pub search: Option<String>,
    /// Opaque cursor from `meta.nextCursor` of the previous page.
    pub cursor: Option<Uuid>,
    /// Page size 1–100 (default 20).
    pub limit: Option<i64>,
}
