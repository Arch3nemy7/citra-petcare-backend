use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;
use validator::Validate;

use super::models::{InventoryCategory, InventoryItem, MovementType, StockMovement};

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
            current_stock: i.current_stock,
            created_at: i.created_at,
            updated_at: i.updated_at,
        }
    }
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
