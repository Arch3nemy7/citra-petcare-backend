use chrono::NaiveDate;
use sqlx::PgPool;

use super::models::{ExpiringItem, LowStockItem, VaccinationDue};
use crate::domain::inventory::models::InventoryCategory;
use crate::error::AppError;

/// Latest vaccination per (patient, vaccine) due on/before `cutoff`.
///
/// `DISTINCT ON` keeps only the most recent row per (patient, vaccine) — an
/// older dose with a passed due date is superseded once a newer dose exists.
/// Due-only records (NULL date_given) sort last, so any administered dose of
/// the same vaccine supersedes its placeholder. The due-date filter must wrap
/// that in a subquery so it applies *after* picking the latest row. sqlx
/// cannot infer nullability through a subquery, hence the `!`/`?` overrides.
pub async fn vaccinations_due(
    db: &PgPool,
    cutoff: NaiveDate,
) -> Result<Vec<VaccinationDue>, AppError> {
    let rows = sqlx::query_as!(
        VaccinationDue,
        r#"
        SELECT latest.vaccination_id AS "vaccination_id!",
               latest.patient_id AS "patient_id!",
               latest.patient_name AS "patient_name!",
               latest.owner_name AS "owner_name?",
               latest.owner_phone AS "owner_phone?",
               latest.vaccine_name AS "vaccine_name!",
               latest.date_given AS "date_given?",
               latest.next_due_date AS "next_due_date!"
        FROM (
            SELECT DISTINCT ON (v.patient_id, v.vaccine_name)
                   v.id AS vaccination_id, v.patient_id, p.name AS patient_name,
                   o.name AS owner_name, o.phone AS owner_phone,
                   v.vaccine_name, v.date_given, v.next_due_date
            FROM vaccinations v
            JOIN patients p ON p.id = v.patient_id
                AND p.deleted_at IS NULL AND p.status = 'ACTIVE'
            LEFT JOIN owners o ON o.id = p.owner_id AND o.deleted_at IS NULL
            WHERE v.deleted_at IS NULL
            ORDER BY v.patient_id, v.vaccine_name, v.date_given DESC NULLS LAST
        ) latest
        WHERE latest.next_due_date IS NOT NULL AND latest.next_due_date <= $1
        ORDER BY latest.next_due_date
        "#,
        cutoff
    )
    .fetch_all(db)
    .await?;
    Ok(rows)
}

/// Items at/below their minimum stock (derived from the movement ledger).
pub async fn low_stock(db: &PgPool) -> Result<Vec<LowStockItem>, AppError> {
    let rows = sqlx::query_as!(
        LowStockItem,
        r#"
        SELECT i.id AS item_id, i.name, i.category AS "category: InventoryCategory",
               i.unit, i.min_stock, COALESCE(m.stock, 0) AS "current_stock!"
        FROM inventory_items i
        LEFT JOIN (
            SELECT item_id,
                   SUM(CASE type WHEN 'IN' THEN qty WHEN 'OUT' THEN -qty ELSE qty END) AS stock
            FROM stock_movements
            WHERE deleted_at IS NULL
            GROUP BY item_id
        ) m ON m.item_id = i.id
        WHERE i.deleted_at IS NULL AND COALESCE(m.stock, 0) <= i.min_stock
        ORDER BY i.name
        "#
    )
    .fetch_all(db)
    .await?;
    Ok(rows)
}

/// Items expiring on/before `cutoff`, optionally restricted to one category
/// (the daily job warns about DRUGs only; the dashboard shows everything).
pub async fn expiring(
    db: &PgPool,
    cutoff: NaiveDate,
    category: Option<InventoryCategory>,
) -> Result<Vec<ExpiringItem>, AppError> {
    let rows = sqlx::query_as!(
        ExpiringItem,
        r#"
        SELECT id AS item_id, name, category AS "category: InventoryCategory", unit,
               expiry_date AS "expiry_date!"
        FROM inventory_items
        WHERE deleted_at IS NULL
          AND expiry_date IS NOT NULL
          AND expiry_date <= $1
          AND ($2::inventory_category IS NULL OR category = $2)
        ORDER BY expiry_date
        "#,
        cutoff,
        category as Option<InventoryCategory>
    )
    .fetch_all(db)
    .await?;
    Ok(rows)
}
