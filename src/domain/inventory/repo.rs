use sqlx::PgPool;
use uuid::Uuid;

use super::InventoryError;
use super::dto::InventoryItemRequest;
use super::models::{InventoryCategory, InventoryItem, MovementType, StockMovement};
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
               i.min_stock, i.expiry_date, COALESCE(m.stock, 0) AS "current_stock!",
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
               i.min_stock, i.expiry_date, COALESCE(m.stock, 0) AS "current_stock!",
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
        INSERT INTO inventory_items (id, name, category, unit, min_stock, expiry_date)
        VALUES ($1, $2, $3, $4, $5, $6)
        "#,
        id,
        input.name,
        input.category as InventoryCategory,
        input.unit,
        input.min_stock,
        input.expiry_date
    )
    .execute(db)
    .await?;
    Ok(())
}

pub async fn upsert(db: &PgPool, id: Uuid, input: &InventoryItemRequest) -> Result<(), AppError> {
    sqlx::query!(
        r#"
        INSERT INTO inventory_items (id, name, category, unit, min_stock, expiry_date)
        VALUES ($1, $2, $3, $4, $5, $6)
        ON CONFLICT (id) DO UPDATE SET
            name = EXCLUDED.name,
            category = EXCLUDED.category,
            unit = EXCLUDED.unit,
            min_stock = EXCLUDED.min_stock,
            expiry_date = EXCLUDED.expiry_date,
            deleted_at = NULL
        "#,
        id,
        input.name,
        input.category as InventoryCategory,
        input.unit,
        input.min_stock,
        input.expiry_date
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
               visit_id, created_at, updated_at
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

/// Insert a movement inside a transaction that (1) row-locks the item so
/// concurrent movements for it serialize, and (2) rejects any movement that
/// would push derived stock below zero. Returns the entry and the new level.
pub async fn record_movement(
    db: &PgPool,
    id: Uuid,
    item_id: Uuid,
    movement_type: MovementType,
    qty: f64,
    reason: Option<&str>,
    visit_id: Option<Uuid>,
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
        INSERT INTO stock_movements (id, item_id, type, qty, reason, visit_id)
        VALUES ($1, $2, $3, $4, $5, $6)
        RETURNING id, item_id, type AS "movement_type: MovementType", qty, reason,
                  visit_id, created_at, updated_at
        "#,
        id,
        item_id,
        movement_type as MovementType,
        qty,
        reason,
        visit_id
    )
    .fetch_one(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok((movement, new_stock))
}
