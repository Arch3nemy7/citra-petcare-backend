use sqlx::PgPool;
use uuid::Uuid;

use super::dto::{BatchUpdateRequest, InventoryItemRequest, MovementRequest};
use super::models::{
    InventoryCategory, InventoryItem, MovementType, StockBatch, StockMovement, allocate_batches,
};
use super::{InventoryError, repo};
use crate::error::AppError;
use crate::http::pagination::Paginated;

pub async fn list(
    db: &PgPool,
    search: Option<&str>,
    category: Option<InventoryCategory>,
    cursor: Option<Uuid>,
    limit: i64,
) -> Result<Paginated<InventoryItem>, AppError> {
    let rows = repo::list(db, search, category, cursor, limit).await?;
    Ok(Paginated::from_rows(rows, limit, |item| item.id))
}

pub async fn get(db: &PgPool, id: Uuid) -> Result<InventoryItem, AppError> {
    repo::find(db, id)
        .await?
        .ok_or(AppError::NotFound("inventory item"))
}

/// The item plus its remaining batches (FEFO allocation of all consumption
/// against expiry-dated stock-ins).
pub async fn get_detail(
    db: &PgPool,
    id: Uuid,
) -> Result<(InventoryItem, Vec<StockBatch>), AppError> {
    let item = get(db, id).await?;
    let batch_ins = repo::batch_ins_for(db, id).await?;
    let batches = if batch_ins.is_empty() {
        Vec::new()
    } else {
        allocate_batches(batch_ins, repo::consumed_total(db, id).await?)
    };
    Ok((item, batches))
}

pub async fn create(db: &PgPool, input: &InventoryItemRequest) -> Result<InventoryItem, AppError> {
    let id = input.id.unwrap_or_else(Uuid::now_v7);
    repo::insert(db, id, input).await?;
    get(db, id).await
}

pub async fn upsert(
    db: &PgPool,
    id: Uuid,
    input: &InventoryItemRequest,
) -> Result<InventoryItem, AppError> {
    repo::upsert(db, id, input).await?;
    get(db, id).await
}

pub async fn delete(db: &PgPool, id: Uuid) -> Result<(), AppError> {
    if !repo::soft_delete(db, id).await? {
        return Err(AppError::NotFound("inventory item"));
    }
    Ok(())
}

/// Correct one batch's expiry date (a data-entry fix). The batch is
/// identified by its current expiry date + lot number; returns the
/// refreshed item + batches so callers can render the new FEFO order.
pub async fn redate_batch(
    db: &PgPool,
    item_id: Uuid,
    input: &BatchUpdateRequest,
) -> Result<(InventoryItem, Vec<StockBatch>), AppError> {
    let matched = repo::redate_batch(
        db,
        item_id,
        input.expiry_date,
        input.lot_no.as_deref(),
        input.new_expiry_date,
    )
    .await?;
    if !matched {
        return Err(AppError::NotFound("stock batch"));
    }
    get_detail(db, item_id).await
}

pub async fn list_movements(
    db: &PgPool,
    item_id: Uuid,
    cursor: Option<Uuid>,
    limit: i64,
) -> Result<Paginated<StockMovement>, AppError> {
    get(db, item_id).await?; // 404 for unknown items
    let rows = repo::movements_for(db, item_id, cursor, limit).await?;
    Ok(Paginated::from_rows(rows, limit, |m| m.id))
}

/// Record a stock movement. Sign rules are enforced here (with friendly
/// messages) and again by a DB CHECK constraint; the no-negative-stock rule
/// runs inside the repo transaction where it is race-free.
pub async fn record_movement(
    db: &PgPool,
    item_id: Uuid,
    input: &MovementRequest,
) -> Result<(StockMovement, f64), AppError> {
    match input.movement_type {
        MovementType::In | MovementType::Out if input.qty <= 0.0 => {
            return Err(InventoryError::InvalidQuantity(
                "qty must be positive for IN/OUT movements".to_string(),
            )
            .into());
        }
        MovementType::Adjustment if input.qty == 0.0 => {
            return Err(InventoryError::InvalidQuantity(
                "qty must be non-zero for an ADJUSTMENT".to_string(),
            )
            .into());
        }
        _ => {}
    }
    let id = input.id.unwrap_or_else(Uuid::now_v7);
    repo::record_movement(
        db,
        id,
        item_id,
        input.movement_type,
        input.qty,
        input.reason.as_deref(),
        input.visit_id,
        input.expiry_date,
        input.lot_no.as_deref(),
    )
    .await
}
