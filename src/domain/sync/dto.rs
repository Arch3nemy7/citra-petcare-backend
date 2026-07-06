use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::domain::appointments::dto::AppointmentResponse;
use crate::domain::inventory::dto::{InventoryItemResponse, MovementResponse};
use crate::domain::owners::dto::OwnerResponse;
use crate::domain::patients::dto::PatientResponse;
use crate::domain::vaccinations::dto::VaccinationResponse;
use crate::domain::visits::dto::{VisitAttachmentResponse, VisitResponse};

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct SyncParams {
    /// RFC 3339 instant of the previous successful pull (exclusive). Use the
    /// `serverTime` from that pull's response.
    pub since: DateTime<Utc>,
}

/// Rows created/updated after `since` (upserts) and rows soft-deleted after
/// `since` (tombstones). Clients apply upserts idempotently and drop local
/// rows named by tombstones.
#[derive(Debug, Serialize, ToSchema)]
pub struct ChangeSet<T> {
    pub upserts: Vec<T>,
    pub tombstones: Vec<Tombstone>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Tombstone {
    pub id: Uuid,
    pub deleted_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ChangesResponse {
    /// Echo of the request parameter.
    pub since: DateTime<Utc>,
    /// Captured *before* querying — pass as `since` on the next pull. A row
    /// updated mid-pull may appear in two consecutive pulls; that is safe
    /// because upserts are idempotent.
    pub server_time: DateTime<Utc>,
    pub owners: ChangeSet<OwnerResponse>,
    pub patients: ChangeSet<PatientResponse>,
    pub visits: ChangeSet<VisitResponse>,
    pub visit_attachments: ChangeSet<VisitAttachmentResponse>,
    pub vaccinations: ChangeSet<VaccinationResponse>,
    pub appointments: ChangeSet<AppointmentResponse>,
    pub inventory_items: ChangeSet<InventoryItemResponse>,
    pub stock_movements: ChangeSet<MovementResponse>,
}
