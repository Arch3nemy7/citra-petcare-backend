use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use super::InventoryError;

/// Stock quantities are `double precision`, so derived sums carry floating-point
/// residue (0.3 - 0.1 - 0.2 lands at -2.8e-17, not 0). Compare against this
/// tolerance instead of exact zero so genuinely-empty ledgers read as empty and
/// exact-drain movements are not spuriously rejected. It sits far below any real
/// quantity yet far above accumulated `f64` dust.
pub const STOCK_EPSILON: f64 = 1e-6;

/// Upper bound on a single movement's magnitude. Guards the derived stock sum
/// against `float8` overflow (two near-`f64::MAX` movements would make every
/// subsequent SUM error out and 500 the item) while staying absurdly high for a
/// two-vet clinic.
pub const MAX_QTY: f64 = 1e9;

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

impl MovementType {
    /// The signed contribution of `qty` to derived stock: IN adds, OUT
    /// subtracts, ADJUSTMENT applies its already-signed value. This is the
    /// Rust mirror of the `CASE type ...` expression the stock queries use.
    pub fn signed(self, qty: f64) -> f64 {
        match self {
            MovementType::In => qty,
            MovementType::Out => -qty,
            MovementType::Adjustment => qty,
        }
    }
}

/// Sign rules shared by recording a movement and correcting its quantity:
/// IN/OUT quantities must be positive; an ADJUSTMENT must be non-zero (it may
/// be signed). Mirrors the `stock_movements_qty_check` DB constraint with a
/// friendlier message.
pub fn check_qty_sign(movement_type: MovementType, qty: f64) -> Result<(), InventoryError> {
    match movement_type {
        MovementType::In | MovementType::Out if qty <= 0.0 => Err(InventoryError::InvalidQuantity(
            "qty must be positive for IN/OUT movements".to_string(),
        )),
        MovementType::Adjustment if qty == 0.0 => Err(InventoryError::InvalidQuantity(
            "qty must be non-zero for an ADJUSTMENT".to_string(),
        )),
        _ => Ok(()),
    }
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
        if qty_remaining > STOCK_EPSILON {
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

    #[test]
    fn floating_point_dust_does_not_survive_as_a_phantom_batch() {
        // IN 0.1 + IN 0.2, then OUT 0.3 nets to exactly zero, but f64 leaves
        // ~2.8e-17 in the last batch. An exact-zero check would keep it and pin
        // the item's expiry badge to a drained lot; the epsilon must drop it.
        let remaining = allocate_batches(
            vec![batch("A", "2026-08-30", 0.1), batch("B", "2027-01-14", 0.2)],
            0.3,
        );
        assert!(remaining.is_empty(), "dust batch survived: {remaining:?}");
    }
}
