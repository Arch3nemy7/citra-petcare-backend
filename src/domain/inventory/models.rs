use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type, ToSchema)]
#[sqlx(type_name = "inventory_category", rename_all = "SCREAMING_SNAKE_CASE")]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum InventoryCategory {
    Drug,
    Vaccine,
    Supply,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type, ToSchema)]
#[sqlx(type_name = "movement_type", rename_all = "SCREAMING_SNAKE_CASE")]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MovementType {
    In,
    Out,
    Adjustment,
}

/// An inventory item. `current_stock` is never stored — every read derives it
/// from stock_movements with an index-only SUM.
#[derive(Debug, Clone)]
pub struct InventoryItem {
    pub id: Uuid,
    pub name: String,
    pub category: InventoryCategory,
    /// Unit of measure shown in the app: botol, vial, pcs, ml, …
    pub unit: String,
    pub min_stock: f64,
    pub expiry_date: Option<NaiveDate>,
    pub current_stock: f64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A stock ledger entry. IN/OUT are positive; ADJUSTMENT may be signed
/// (e.g. -2 after breakage or a recount).
#[derive(Debug, Clone)]
pub struct StockMovement {
    pub id: Uuid,
    pub item_id: Uuid,
    pub movement_type: MovementType,
    pub qty: f64,
    pub reason: Option<String>,
    /// Set when the stock was used during a visit.
    pub visit_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
