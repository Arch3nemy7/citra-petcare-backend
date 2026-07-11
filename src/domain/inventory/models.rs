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
    /// Package/label photos (storage keys), captured at registration.
    pub photo_keys: Vec<String>,
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
    /// An IN movement with an expiry date opens a batch (lot) of that date.
    pub expiry_date: Option<NaiveDate>,
    pub lot_no: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A stock-in that opened a batch, as read back from the ledger.
#[derive(Debug, Clone)]
pub struct BatchIn {
    pub lot_no: Option<String>,
    pub expiry_date: NaiveDate,
    pub qty: f64,
}

/// A batch with stock still on the shelf, derived from the ledger.
#[derive(Debug, Clone, PartialEq)]
pub struct StockBatch {
    pub lot_no: Option<String>,
    pub expiry_date: NaiveDate,
    pub qty_remaining: f64,
}

/// FEFO allocation: consumption is charged against batches earliest-expiry
/// first (the clinic policy the batch list documents), so `qty_remaining`
/// reflects what is left of each lot. `batches` must be sorted by expiry
/// (then receipt time) ascending — the order `batch_ins_for` returns.
pub fn allocate_batches(batches: Vec<BatchIn>, consumed: f64) -> Vec<StockBatch> {
    let mut left = consumed.max(0.0);
    let mut remaining = Vec::new();
    for batch in batches {
        let used = left.min(batch.qty);
        left -= used;
        let qty_remaining = batch.qty - used;
        if qty_remaining > 0.0 {
            remaining.push(StockBatch {
                lot_no: batch.lot_no,
                expiry_date: batch.expiry_date,
                qty_remaining,
            });
        }
    }
    remaining
}

#[cfg(test)]
mod tests {
    use super::*;

    fn batch(lot: &str, expiry: &str, qty: f64) -> BatchIn {
        BatchIn {
            lot_no: Some(lot.to_string()),
            expiry_date: expiry.parse().expect("valid date"),
            qty,
        }
    }

    #[test]
    fn consumption_drains_earliest_expiry_first() {
        let remaining = allocate_batches(
            vec![batch("A", "2026-08-30", 6.0), batch("B", "2027-01-14", 8.0)],
            4.0,
        );
        assert_eq!(remaining.len(), 2);
        assert_eq!(remaining[0].qty_remaining, 2.0);
        assert_eq!(remaining[1].qty_remaining, 8.0);
    }

    #[test]
    fn fully_consumed_batches_are_dropped() {
        let remaining = allocate_batches(
            vec![batch("A", "2026-08-30", 6.0), batch("B", "2027-01-14", 8.0)],
            6.0,
        );
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].lot_no.as_deref(), Some("B"));
        assert_eq!(remaining[0].qty_remaining, 8.0);
    }

    #[test]
    fn overconsumption_beyond_batches_yields_empty() {
        let remaining = allocate_batches(vec![batch("A", "2026-08-30", 6.0)], 10.0);
        assert!(remaining.is_empty());
    }
}
