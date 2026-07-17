use chrono::NaiveDate;
use sqlx::{PgExecutor, PgPool};
use uuid::Uuid;

use super::InventoryError;
use super::dto::InventoryItemRequest;
use super::models::{
    BatchIn, InventoryCategory, InventoryItem, MovementType, STOCK_EPSILON, StockMovement,
    allocate_batches, check_qty_sign,
};
use crate::error::AppError;

// The "current stock" expression: IN adds, OUT subtracts, ADJUSTMENT applies
// its signed qty. It aggregates over the covering partial index
// stock_movements_item_id_idx, so it stays an index-only scan.

/// Lock the item row for the rest of the caller's transaction so concurrent
/// stock writes serialize, returning NotFound (and rolling the transaction back
/// on drop) when the item is absent or soft-deleted.
async fn lock_item(conn: &mut sqlx::PgConnection, item_id: Uuid) -> Result<(), AppError> {
    let locked = sqlx::query_scalar!(
        "SELECT id FROM inventory_items WHERE id = $1 AND deleted_at IS NULL FOR UPDATE",
        item_id
    )
    .fetch_optional(&mut *conn)
    .await?;
    if locked.is_none() {
        return Err(AppError::NotFound("inventory item")); // dropping tx rolls back
    }
    Ok(())
}

/// Re-derive the item's expiry badge (earliest remaining FEFO batch) from the
/// ledger and write it, bumping the sync cursor via the update trigger. Returns
/// `false` without touching the row when the item tracks no expiry-dated
/// batches — the badge is then client-owned, and the caller decides whether the
/// ledger change still needs the sync cursor moved.
async fn refresh_expiry_badge(
    conn: &mut sqlx::PgConnection,
    item_id: Uuid,
) -> Result<bool, AppError> {
    let batch_ins = batch_ins_for(&mut *conn, item_id).await?;
    if batch_ins.is_empty() {
        return Ok(false);
    }
    let consumed = consumed_total(&mut *conn, item_id).await?;
    let next_expiry = allocate_batches(batch_ins, consumed)
        .first()
        .map(|batch| batch.expiry_date);
    sqlx::query!(
        "UPDATE inventory_items SET expiry_date = $2 WHERE id = $1",
        item_id,
        next_expiry
    )
    .execute(&mut *conn)
    .await?;
    Ok(true)
}

/// Move the item's sync cursor (`updated_at`, via the update trigger) without
/// altering any other field — used when a ledger write leaves the item row
/// otherwise untouched (an item with no expiry-dated batches), so
/// `/sync/changes` still re-emits the item with its freshly-derived stock.
async fn touch_item(conn: &mut sqlx::PgConnection, item_id: Uuid) -> Result<(), AppError> {
    sqlx::query!(
        "UPDATE inventory_items SET updated_at = now() WHERE id = $1",
        item_id
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

pub async fn list(
    db: &PgPool,
    search: Option<&str>,
    category: Option<InventoryCategory>,
    cursor: Option<Uuid>,
    limit: i64,
) -> Result<Vec<InventoryItem>, AppError> {
    // Escape LIKE metacharacters so `%`/`_`/`\` in the term match literally.
    let search = search.map(crate::db::escape_like);
    let rows = sqlx::query_as!(
        InventoryItem,
        r#"
        SELECT i.id, i.name, i.category AS "category: InventoryCategory", i.unit,
               i.min_stock, i.expiry_date, i.photo_keys,
               COALESCE(m.stock, 0) AS "current_stock!",
               i.created_at, i.updated_at
        FROM inventory_items i
        LEFT JOIN LATERAL (
            SELECT SUM(CASE type WHEN 'IN' THEN qty WHEN 'OUT' THEN -qty ELSE qty END) AS stock
            FROM stock_movements
            WHERE item_id = i.id AND deleted_at IS NULL
        ) m ON true
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

pub async fn find<'e>(
    executor: impl PgExecutor<'e>,
    id: Uuid,
) -> Result<Option<InventoryItem>, AppError> {
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
    .fetch_optional(executor)
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
    let mut tx = db.begin().await?;
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
    .execute(&mut *tx)
    .await?;
    // Upserting a full representation may resurrect or edit an item that already
    // tracks batches; there the client-sent expiry_date is not authoritative, so
    // re-derive the badge from the ledger rather than let the payload clobber it.
    refresh_expiry_badge(&mut tx, id).await?;
    tx.commit().await?;
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
    lock_item(&mut tx, item_id).await?;

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

    let new_stock = current + movement_type.signed(qty);
    if new_stock < -STOCK_EPSILON {
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

    if !refresh_expiry_badge(&mut tx, item_id).await? {
        // No expiry-dated batches to re-derive a badge from, so the ledger write
        // left the item row untouched; bump its sync cursor explicitly so
        // /sync/changes re-emits the item with its new derived stock.
        touch_item(&mut tx, item_id).await?;
    }

    tx.commit().await?;
    Ok((movement, new_stock))
}

/// Correct a movement's quantity inside an item-locked transaction: the
/// same no-negative-stock rule as recording applies, evaluated as if the
/// entry had been written with the new qty all along. Visit-linked
/// movements are rejected — they must stay consistent with their visit's
/// stock usage. Refreshes the expiry badge (an edited stock-in changes
/// what remains of its batch).
pub async fn update_movement_qty(
    db: &PgPool,
    item_id: Uuid,
    movement_id: Uuid,
    qty: f64,
) -> Result<(StockMovement, f64), AppError> {
    let mut tx = db.begin().await?;
    lock_item(&mut tx, item_id).await?;

    let existing = sqlx::query!(
        r#"
        SELECT type AS "movement_type: MovementType", qty, visit_id
        FROM stock_movements
        WHERE id = $1 AND item_id = $2 AND deleted_at IS NULL
        "#,
        movement_id,
        item_id
    )
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(AppError::NotFound("stock movement"))?;

    if existing.visit_id.is_some() {
        return Err(InventoryError::VisitLinkedMovement.into());
    }
    check_qty_sign(existing.movement_type, qty)?;

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

    let without_entry = current - existing.movement_type.signed(existing.qty);
    let new_stock = without_entry + existing.movement_type.signed(qty);
    if new_stock < -STOCK_EPSILON {
        return Err(InventoryError::InsufficientStock {
            requested: qty.abs(),
            available: without_entry,
        }
        .into());
    }

    let movement = sqlx::query_as!(
        StockMovement,
        r#"
        UPDATE stock_movements
        SET qty = $3
        WHERE id = $1 AND item_id = $2
        RETURNING id, item_id, type AS "movement_type: MovementType", qty, reason,
                  visit_id, expiry_date, lot_no, created_at, updated_at
        "#,
        movement_id,
        item_id,
        qty
    )
    .fetch_one(&mut *tx)
    .await?;

    if !refresh_expiry_badge(&mut tx, item_id).await? {
        // No expiry-dated batches to re-derive a badge from, so the ledger write
        // left the item row untouched; bump its sync cursor explicitly so
        // /sync/changes re-emits the item with its new derived stock.
        touch_item(&mut tx, item_id).await?;
    }

    tx.commit().await?;
    Ok((movement, new_stock))
}

/// Move one batch to a new expiry date: re-date every stock-in matching the
/// batch key (current expiry + lot), then refresh the item's expiry badge to
/// the earliest remaining batch — all inside one item-locked transaction so
/// concurrent movements serialize against it. Returns false when no
/// stock-in matched (unknown batch).
pub async fn redate_batch(
    db: &PgPool,
    item_id: Uuid,
    expiry_date: NaiveDate,
    lot_no: Option<&str>,
    new_expiry_date: NaiveDate,
) -> Result<bool, AppError> {
    let mut tx = db.begin().await?;
    lock_item(&mut tx, item_id).await?;

    // Re-dating a batch onto another batch's (expiry, lot) key would merge two
    // physically distinct lots irreversibly. Reject that, unless the target date
    // equals the source (a no-op that can only match the batch itself).
    if new_expiry_date != expiry_date {
        let collides = sqlx::query_scalar!(
            r#"
            SELECT EXISTS(
                SELECT 1 FROM stock_movements
                WHERE item_id = $1 AND deleted_at IS NULL AND type = 'IN'
                  AND expiry_date = $2 AND lot_no IS NOT DISTINCT FROM $3
            ) AS "exists!"
            "#,
            item_id,
            new_expiry_date,
            lot_no
        )
        .fetch_one(&mut *tx)
        .await?;
        if collides {
            return Err(InventoryError::BatchConflict.into());
        }
    }

    let updated = sqlx::query!(
        r#"
        UPDATE stock_movements
        SET expiry_date = $4
        WHERE item_id = $1 AND deleted_at IS NULL AND type = 'IN'
          AND expiry_date = $2 AND lot_no IS NOT DISTINCT FROM $3
        "#,
        item_id,
        expiry_date,
        lot_no,
        new_expiry_date
    )
    .execute(&mut *tx)
    .await?;
    if updated.rows_affected() == 0 {
        return Ok(false);
    }

    // At least one expiry-dated stock-in exists (we just re-dated it), so the
    // badge refresh always has a batch to allocate and touches the row.
    refresh_expiry_badge(&mut tx, item_id).await?;

    tx.commit().await?;
    Ok(true)
}
