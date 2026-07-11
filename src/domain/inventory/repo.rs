use chrono::NaiveDate;
use sqlx::{PgExecutor, PgPool};
use uuid::Uuid;

use super::InventoryError;
use super::dto::InventoryItemRequest;
use super::models::{
    BatchIn, InventoryCategory, InventoryItem, MovementType, StockMovement, allocate_batches,
};
use crate::error::AppError;

// The "current stock" expression: IN adds, OUT subtracts, ADJUSTMENT applies
// its signed qty. It aggregates over the covering partial index
// stock_movements_item_id_idx, so it stays an index-only scan.

pub async fn list(
    db: &PgPool,
    search: Option<&str>,
    category: Option<InventoryCategory>,
    cursor: Option<Uuid>,
    limit: i64,
) -> Result<Vec<InventoryItem>, AppError> {
    let rows = sqlx::query_as!(
        InventoryItem,
        r#"
        SELECT i.id, i.name, i.category AS "category: InventoryCategory", i.unit,
               i.min_stock, i.expiry_date, i.photo_keys,
               COALESCE(m.stock, 0) AS "current_stock!",
               i.created_at, i.updated_at
        FROM inventory_items i
        LEFT JOIN (
            SELECT item_id,
                   SUM(CASE type WHEN 'IN' THEN qty WHEN 'OUT' THEN -qty ELSE qty END) AS stock
            FROM stock_movements
            WHERE deleted_at IS NULL
            GROUP BY item_id
        ) m ON m.item_id = i.id
        WHERE i.deleted_at IS NULL
          AND ($1::text IS NULL OR i.name ILIKE '%' || $1 || '%')
          AND ($2::inventory_category IS NULL OR i.category = $2)
          AND ($3::uuid IS NULL OR i.id < $3)
        ORDER BY i.id DESC
        LIMIT $4
        "#,
        search,
        category as Option<InventoryCategory>,
        cursor,
        limit + 1
    )
    .fetch_all(db)
    .await?;
    Ok(rows)
}

pub async fn find(db: &PgPool, id: Uuid) -> Result<Option<InventoryItem>, AppError> {
    let item = sqlx::query_as!(
        InventoryItem,
        r#"
        SELECT i.id, i.name, i.category AS "category: InventoryCategory", i.unit,
               i.min_stock, i.expiry_date, i.photo_keys,
               COALESCE(m.stock, 0) AS "current_stock!",
               i.created_at, i.updated_at
        FROM inventory_items i
        LEFT JOIN (
            SELECT item_id,
                   SUM(CASE type WHEN 'IN' THEN qty WHEN 'OUT' THEN -qty ELSE qty END) AS stock
            FROM stock_movements
            WHERE deleted_at IS NULL
            GROUP BY item_id
        ) m ON m.item_id = i.id
        WHERE i.id = $1 AND i.deleted_at IS NULL
        "#,
        id
    )
    .fetch_optional(db)
    .await?;
    Ok(item)
}

pub async fn insert(db: &PgPool, id: Uuid, input: &InventoryItemRequest) -> Result<(), AppError> {
    sqlx::query!(
        r#"
        INSERT INTO inventory_items (id, name, category, unit, min_stock, expiry_date, photo_keys)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        "#,
        id,
        input.name,
        input.category as InventoryCategory,
        input.unit,
        input.min_stock,
        input.expiry_date,
        &input.photo_keys
    )
    .execute(db)
    .await?;
    Ok(())
}

pub async fn upsert(db: &PgPool, id: Uuid, input: &InventoryItemRequest) -> Result<(), AppError> {
    sqlx::query!(
        r#"
        INSERT INTO inventory_items (id, name, category, unit, min_stock, expiry_date, photo_keys)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        ON CONFLICT (id) DO UPDATE SET
            name = EXCLUDED.name,
            category = EXCLUDED.category,
            unit = EXCLUDED.unit,
            min_stock = EXCLUDED.min_stock,
            expiry_date = EXCLUDED.expiry_date,
            photo_keys = EXCLUDED.photo_keys,
            deleted_at = NULL
        "#,
        id,
        input.name,
        input.category as InventoryCategory,
        input.unit,
        input.min_stock,
        input.expiry_date,
        &input.photo_keys
    )
    .execute(db)
    .await?;
    Ok(())
}

pub async fn soft_delete(db: &PgPool, id: Uuid) -> Result<bool, AppError> {
    let result = sqlx::query!(
        "UPDATE inventory_items SET deleted_at = now() WHERE id = $1 AND deleted_at IS NULL",
        id
    )
    .execute(db)
    .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn movements_for(
    db: &PgPool,
    item_id: Uuid,
    cursor: Option<Uuid>,
    limit: i64,
) -> Result<Vec<StockMovement>, AppError> {
    let rows = sqlx::query_as!(
        StockMovement,
        r#"
        SELECT id, item_id, type AS "movement_type: MovementType", qty, reason,
               visit_id, expiry_date, lot_no, created_at, updated_at
        FROM stock_movements
        WHERE item_id = $1 AND deleted_at IS NULL
          AND ($2::uuid IS NULL OR id < $2)
        ORDER BY id DESC
        LIMIT $3
        "#,
        item_id,
        cursor,
        limit + 1
    )
    .fetch_all(db)
    .await?;
    Ok(rows)
}

/// Stock-ins that opened a batch, earliest expiry (then receipt) first — the
/// order `allocate_batches` expects.
pub async fn batch_ins_for<'e>(
    executor: impl PgExecutor<'e>,
    item_id: Uuid,
) -> Result<Vec<BatchIn>, AppError> {
    let rows = sqlx::query_as!(
        BatchIn,
        r#"
        SELECT lot_no, expiry_date AS "expiry_date!", qty
        FROM stock_movements
        WHERE item_id = $1 AND deleted_at IS NULL
          AND type = 'IN' AND expiry_date IS NOT NULL
        ORDER BY expiry_date, created_at
        "#,
        item_id
    )
    .fetch_all(executor)
    .await?;
    Ok(rows)
}

/// Everything ever taken off the shelf: OUT movements plus negative
/// adjustments (breakage, recount). Feeds the FEFO allocation.
pub async fn consumed_total<'e>(
    executor: impl PgExecutor<'e>,
    item_id: Uuid,
) -> Result<f64, AppError> {
    let consumed = sqlx::query_scalar!(
        r#"
        SELECT COALESCE(SUM(CASE
            WHEN type = 'OUT' THEN qty
            WHEN type = 'ADJUSTMENT' AND qty < 0 THEN -qty
            ELSE 0
        END), 0) AS "consumed!"
        FROM stock_movements
        WHERE item_id = $1 AND deleted_at IS NULL
        "#,
        item_id
    )
    .fetch_one(executor)
    .await?;
    Ok(consumed)
}

/// Insert a movement inside a transaction that (1) row-locks the item so
/// concurrent movements for it serialize, and (2) rejects any movement that
/// would push derived stock below zero. When the item tracks batches, its
/// expiry badge is refreshed to the earliest remaining batch in the same
/// transaction. Returns the entry and the new stock level.
#[allow(clippy::too_many_arguments)] // flat argument list keeps the query obvious
pub async fn record_movement(
    db: &PgPool,
    id: Uuid,
    item_id: Uuid,
    movement_type: MovementType,
    qty: f64,
    reason: Option<&str>,
    visit_id: Option<Uuid>,
    expiry_date: Option<NaiveDate>,
    lot_no: Option<&str>,
) -> Result<(StockMovement, f64), AppError> {
    let mut tx = db.begin().await?;

    let locked = sqlx::query_scalar!(
        "SELECT id FROM inventory_items WHERE id = $1 AND deleted_at IS NULL FOR UPDATE",
        item_id
    )
    .fetch_optional(&mut *tx)
    .await?;
    if locked.is_none() {
        return Err(AppError::NotFound("inventory item")); // dropping tx rolls back
    }

    let current: f64 = sqlx::query_scalar!(
        r#"
        SELECT COALESCE(SUM(CASE type WHEN 'IN' THEN qty WHEN 'OUT' THEN -qty ELSE qty END), 0)
            AS "stock!"
        FROM stock_movements
        WHERE item_id = $1 AND deleted_at IS NULL
        "#,
        item_id
    )
    .fetch_one(&mut *tx)
    .await?;

    let delta = match movement_type {
        MovementType::In => qty,
        MovementType::Out => -qty,
        MovementType::Adjustment => qty,
    };
    let new_stock = current + delta;
    if new_stock < 0.0 {
        return Err(InventoryError::InsufficientStock {
            requested: qty.abs(),
            available: current,
        }
        .into());
    }

    let movement = sqlx::query_as!(
        StockMovement,
        r#"
        INSERT INTO stock_movements (id, item_id, type, qty, reason, visit_id, expiry_date, lot_no)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        RETURNING id, item_id, type AS "movement_type: MovementType", qty, reason,
                  visit_id, expiry_date, lot_no, created_at, updated_at
        "#,
        id,
        item_id,
        movement_type as MovementType,
        qty,
        reason,
        visit_id,
        expiry_date,
        lot_no
    )
    .fetch_one(&mut *tx)
    .await?;

    let batch_ins = batch_ins_for(&mut *tx, item_id).await?;
    if !batch_ins.is_empty() {
        let consumed = consumed_total(&mut *tx, item_id).await?;
        let next_expiry = allocate_batches(batch_ins, consumed)
            .first()
            .map(|batch| batch.expiry_date);
        sqlx::query!(
            "UPDATE inventory_items SET expiry_date = $2 WHERE id = $1",
            item_id,
            next_expiry
        )
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok((movement, new_stock))
}
