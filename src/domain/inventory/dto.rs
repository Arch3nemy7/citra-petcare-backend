use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;
use validator::Validate;

use super::models::{InventoryCategory, InventoryItem, MovementType, StockBatch, StockMovement};

/// Body for both POST (create) and PUT (idempotent upsert).
#[derive(Debug, Deserialize, Validate, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct InventoryItemRequest {
    /// Client-generated UUIDv7. Optional on POST; ignored on PUT (path wins).
    pub id: Option<Uuid>,
    #[validate(length(min = 1, max = 200))]
    #[schema(example = "Amoxicillin Sirup Kering 60 ml")]
    pub name: String,
    pub category: InventoryCategory,
    #[validate(length(min = 1, max = 20))]
    #[schema(example = "botol")]
    pub unit: String,
    /// Dashboard/scheduler flag threshold.
    #[validate(range(min = 0.0))]
    #[serde(default)]
    pub min_stock: f64,
    pub expiry_date: Option<NaiveDate>,
    /// Package/label photo storage keys (from /storage/presign-upload).
    #[validate(length(max = 3), custom(function = "validate_photo_keys"))]
    #[serde(default)]
    pub photo_keys: Vec<String>,
}

fn validate_photo_keys(keys: &[String]) -> Result<(), validator::ValidationError> {
    if keys.iter().any(|k| k.is_empty() || k.len() > 500) {
        return Err(validator::ValidationError::new("photo_keys")
            .with_message("each photo key must be 1–500 characters".into()));
    }
    Ok(())
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct InventoryItemResponse {
    pub id: Uuid,
    pub name: String,
    pub category: InventoryCategory,
    pub unit: String,
    pub min_stock: f64,
    pub expiry_date: Option<NaiveDate>,
    pub photo_keys: Vec<String>,
    /// Derived from the movement ledger, never stored.
    pub current_stock: f64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<InventoryItem> for InventoryItemResponse {
    fn from(i: InventoryItem) -> Self {
        Self {
            id: i.id,
            name: i.name,
            category: i.category,
            unit: i.unit,
            min_stock: i.min_stock,
            expiry_date: i.expiry_date,
            photo_keys: i.photo_keys,
            current_stock: i.current_stock,
            created_at: i.created_at,
            updated_at: i.updated_at,
        }
    }
}

/// One lot with stock still on the shelf, earliest expiry first (FEFO).
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct StockBatchResponse {
    pub lot_no: Option<String>,
    pub expiry_date: NaiveDate,
    /// What is left of the lot after FEFO-allocating all consumption.
    pub qty_remaining: f64,
}

impl From<StockBatch> for StockBatchResponse {
    fn from(b: StockBatch) -> Self {
        Self {
            lot_no: b.lot_no,
            expiry_date: b.expiry_date,
            qty_remaining: b.qty_remaining,
        }
    }
}

/// GET /inventory/items/{id}: the item plus its remaining batches.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct InventoryItemDetailResponse {
    #[serde(flatten)]
    pub item: InventoryItemResponse,
    /// Empty when stock was never received with an expiry date.
    pub batches: Vec<StockBatchResponse>,
}

/// Body for PATCH /inventory/items/{id}/batches: correct one batch's expiry
/// date. The batch is identified by its current expiry date + lot number
/// (the same pair the batch list shows); every stock-in that opened it is
/// re-dated, so the derived batch moves as a whole.
#[derive(Debug, Deserialize, Validate, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BatchUpdateRequest {
    /// Current expiry date of the batch.
    pub expiry_date: NaiveDate,
    /// Lot number of the batch; omit for batches received without one.
    #[validate(length(min = 1, max = 100))]
    pub lot_no: Option<String>,
    pub new_expiry_date: NaiveDate,
}

#[derive(Debug, Deserialize, Validate, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MovementRequest {
    /// Client-generated UUIDv7 (optional).
    pub id: Option<Uuid>,
    #[serde(rename = "type")]
    pub movement_type: MovementType,
    /// Positive for IN/OUT; ADJUSTMENT accepts a signed delta.
    pub qty: f64,
    #[validate(length(max = 500))]
    #[schema(example = "Pembelian dari supplier")]
    pub reason: Option<String>,
    /// Link to the visit that consumed the stock (optional).
    pub visit_id: Option<Uuid>,
    /// For IN movements: expiry date of the received batch. Opens a batch in
    /// the item's FEFO list and keeps the item's expiry badge current.
    pub expiry_date: Option<NaiveDate>,
    /// Supplier lot/batch code of the received stock (optional).
    #[validate(length(max = 100))]
    pub lot_no: Option<String>,
}

/// Body for PATCH /inventory/items/{id}/movements/{movementId}: correct a
/// mistyped quantity (e.g. 90 entered instead of 9). Sign rules for the
/// movement's type still apply, and the edit is rejected when it would
/// drive derived stock negative. Movements recorded by a visit cannot be
/// edited here — correct those from the visit.
#[derive(Debug, Deserialize, Validate, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MovementUpdateRequest {
    /// Positive for IN/OUT; ADJUSTMENT accepts a signed delta.
    pub qty: f64,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MovementResponse {
    pub id: Uuid,
    pub item_id: Uuid,
    #[serde(rename = "type")]
    pub movement_type: MovementType,
    pub qty: f64,
    pub reason: Option<String>,
    pub visit_id: Option<Uuid>,
    pub expiry_date: Option<NaiveDate>,
    pub lot_no: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<StockMovement> for MovementResponse {
    fn from(m: StockMovement) -> Self {
        Self {
            id: m.id,
            item_id: m.item_id,
            movement_type: m.movement_type,
            qty: m.qty,
            reason: m.reason,
            visit_id: m.visit_id,
            expiry_date: m.expiry_date,
            lot_no: m.lot_no,
            created_at: m.created_at,
            updated_at: m.updated_at,
        }
    }
}

/// Response to recording a movement: the ledger entry plus the item's stock
/// level after it.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RecordedMovementResponse {
    #[serde(flatten)]
    pub movement: MovementResponse,
    pub current_stock: f64,
}

#[derive(Debug, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct ListItemsParams {
    /// Case-insensitive substring match on the item name.
    pub search: Option<String>,
    pub category: Option<InventoryCategory>,
    pub cursor: Option<Uuid>,
    pub limit: Option<i64>,
}
