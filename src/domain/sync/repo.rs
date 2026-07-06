//! Change-feed queries: for each synced entity, rows *updated* after `since`
//! split into live rows (upserts) and soft-deleted rows (tombstones). Every
//! table has an index on updated_at, so these are range scans.

use chrono::{DateTime, Utc};
use sqlx::PgPool;

use super::dto::Tombstone;
use crate::domain::appointments::models::{Appointment, AppointmentStatus};
use crate::domain::inventory::models::{
    InventoryCategory, InventoryItem, MovementType, StockMovement,
};
use crate::domain::owners::models::Owner;
use crate::domain::patients::models::{Patient, PatientStatus, Sex, Species};
use crate::domain::vaccinations::models::Vaccination;
use crate::domain::visits::models::{AttachmentKind, Visit, VisitAttachment};
use crate::error::AppError;

pub async fn owners_changed(db: &PgPool, since: DateTime<Utc>) -> Result<Vec<Owner>, AppError> {
    Ok(sqlx::query_as!(
        Owner,
        r#"
        SELECT id, name, phone, alt_phone, address, notes, created_at, updated_at
        FROM owners WHERE deleted_at IS NULL AND updated_at > $1 ORDER BY updated_at
        "#,
        since
    )
    .fetch_all(db)
    .await?)
}

pub async fn owners_tombstones(
    db: &PgPool,
    since: DateTime<Utc>,
) -> Result<Vec<Tombstone>, AppError> {
    Ok(sqlx::query_as!(
        Tombstone,
        r#"SELECT id, deleted_at AS "deleted_at!" FROM owners
           WHERE deleted_at IS NOT NULL AND updated_at > $1 ORDER BY updated_at"#,
        since
    )
    .fetch_all(db)
    .await?)
}

pub async fn patients_changed(db: &PgPool, since: DateTime<Utc>) -> Result<Vec<Patient>, AppError> {
    Ok(sqlx::query_as!(
        Patient,
        r#"
        SELECT p.id, p.owner_id, o.name AS "owner_name!", p.name,
               p.species AS "species: Species", p.breed, p.sex AS "sex: Sex",
               p.sterilized, p.birth_date, p.color_markings, p.microchip_no,
               p.photo_key, p.allergies, p.alert_notes,
               p.status AS "status: PatientStatus", p.created_at, p.updated_at
        FROM patients p
        JOIN owners o ON o.id = p.owner_id
        WHERE p.deleted_at IS NULL AND p.updated_at > $1 ORDER BY p.updated_at
        "#,
        since
    )
    .fetch_all(db)
    .await?)
}

pub async fn patients_tombstones(
    db: &PgPool,
    since: DateTime<Utc>,
) -> Result<Vec<Tombstone>, AppError> {
    Ok(sqlx::query_as!(
        Tombstone,
        r#"SELECT id, deleted_at AS "deleted_at!" FROM patients
           WHERE deleted_at IS NOT NULL AND updated_at > $1 ORDER BY updated_at"#,
        since
    )
    .fetch_all(db)
    .await?)
}

pub async fn visits_changed(db: &PgPool, since: DateTime<Utc>) -> Result<Vec<Visit>, AppError> {
    Ok(sqlx::query_as!(
        Visit,
        r#"
        SELECT v.id, v.patient_id, p.name AS "patient_name!", v.vet_id,
               u.name AS "vet_name!", v.visit_date, v.complaint, v.temperature_c,
               v.weight_kg, v.exam_notes, v.diagnosis, v.treatment,
               v.prescription, v.follow_up_date, v.created_at, v.updated_at
        FROM visits v
        JOIN patients p ON p.id = v.patient_id
        JOIN users u ON u.id = v.vet_id
        WHERE v.deleted_at IS NULL AND v.updated_at > $1 ORDER BY v.updated_at
        "#,
        since
    )
    .fetch_all(db)
    .await?)
}

pub async fn visits_tombstones(
    db: &PgPool,
    since: DateTime<Utc>,
) -> Result<Vec<Tombstone>, AppError> {
    Ok(sqlx::query_as!(
        Tombstone,
        r#"SELECT id, deleted_at AS "deleted_at!" FROM visits
           WHERE deleted_at IS NOT NULL AND updated_at > $1 ORDER BY updated_at"#,
        since
    )
    .fetch_all(db)
    .await?)
}

pub async fn attachments_changed(
    db: &PgPool,
    since: DateTime<Utc>,
) -> Result<Vec<VisitAttachment>, AppError> {
    Ok(sqlx::query_as!(
        VisitAttachment,
        r#"
        SELECT id, visit_id, file_key, kind AS "kind: AttachmentKind", created_at, updated_at
        FROM visit_attachments
        WHERE deleted_at IS NULL AND updated_at > $1 ORDER BY updated_at
        "#,
        since
    )
    .fetch_all(db)
    .await?)
}

pub async fn attachments_tombstones(
    db: &PgPool,
    since: DateTime<Utc>,
) -> Result<Vec<Tombstone>, AppError> {
    Ok(sqlx::query_as!(
        Tombstone,
        r#"SELECT id, deleted_at AS "deleted_at!" FROM visit_attachments
           WHERE deleted_at IS NOT NULL AND updated_at > $1 ORDER BY updated_at"#,
        since
    )
    .fetch_all(db)
    .await?)
}

pub async fn vaccinations_changed(
    db: &PgPool,
    since: DateTime<Utc>,
) -> Result<Vec<Vaccination>, AppError> {
    Ok(sqlx::query_as!(
        Vaccination,
        r#"
        SELECT v.id, v.patient_id, p.name AS "patient_name!", v.visit_id,
               v.vaccine_name, v.date_given, v.batch_no, v.next_due_date,
               v.created_at, v.updated_at
        FROM vaccinations v
        JOIN patients p ON p.id = v.patient_id
        WHERE v.deleted_at IS NULL AND v.updated_at > $1 ORDER BY v.updated_at
        "#,
        since
    )
    .fetch_all(db)
    .await?)
}

pub async fn vaccinations_tombstones(
    db: &PgPool,
    since: DateTime<Utc>,
) -> Result<Vec<Tombstone>, AppError> {
    Ok(sqlx::query_as!(
        Tombstone,
        r#"SELECT id, deleted_at AS "deleted_at!" FROM vaccinations
           WHERE deleted_at IS NOT NULL AND updated_at > $1 ORDER BY updated_at"#,
        since
    )
    .fetch_all(db)
    .await?)
}

pub async fn appointments_changed(
    db: &PgPool,
    since: DateTime<Utc>,
) -> Result<Vec<Appointment>, AppError> {
    Ok(sqlx::query_as!(
        Appointment,
        r#"
        SELECT a.id, a.patient_id, p.name AS "patient_name!", o.name AS "owner_name!",
               a.scheduled_at, a.reason, a.status AS "status: AppointmentStatus",
               a.notes, a.created_at, a.updated_at
        FROM appointments a
        JOIN patients p ON p.id = a.patient_id
        JOIN owners o ON o.id = p.owner_id
        WHERE a.deleted_at IS NULL AND a.updated_at > $1 ORDER BY a.updated_at
        "#,
        since
    )
    .fetch_all(db)
    .await?)
}

pub async fn appointments_tombstones(
    db: &PgPool,
    since: DateTime<Utc>,
) -> Result<Vec<Tombstone>, AppError> {
    Ok(sqlx::query_as!(
        Tombstone,
        r#"SELECT id, deleted_at AS "deleted_at!" FROM appointments
           WHERE deleted_at IS NOT NULL AND updated_at > $1 ORDER BY updated_at"#,
        since
    )
    .fetch_all(db)
    .await?)
}

pub async fn items_changed(
    db: &PgPool,
    since: DateTime<Utc>,
) -> Result<Vec<InventoryItem>, AppError> {
    Ok(sqlx::query_as!(
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
        WHERE i.deleted_at IS NULL AND i.updated_at > $1 ORDER BY i.updated_at
        "#,
        since
    )
    .fetch_all(db)
    .await?)
}

pub async fn items_tombstones(
    db: &PgPool,
    since: DateTime<Utc>,
) -> Result<Vec<Tombstone>, AppError> {
    Ok(sqlx::query_as!(
        Tombstone,
        r#"SELECT id, deleted_at AS "deleted_at!" FROM inventory_items
           WHERE deleted_at IS NOT NULL AND updated_at > $1 ORDER BY updated_at"#,
        since
    )
    .fetch_all(db)
    .await?)
}

pub async fn movements_changed(
    db: &PgPool,
    since: DateTime<Utc>,
) -> Result<Vec<StockMovement>, AppError> {
    Ok(sqlx::query_as!(
        StockMovement,
        r#"
        SELECT id, item_id, type AS "movement_type: MovementType", qty, reason,
               visit_id, created_at, updated_at
        FROM stock_movements
        WHERE deleted_at IS NULL AND updated_at > $1 ORDER BY updated_at
        "#,
        since
    )
    .fetch_all(db)
    .await?)
}

pub async fn movements_tombstones(
    db: &PgPool,
    since: DateTime<Utc>,
) -> Result<Vec<Tombstone>, AppError> {
    Ok(sqlx::query_as!(
        Tombstone,
        r#"SELECT id, deleted_at AS "deleted_at!" FROM stock_movements
           WHERE deleted_at IS NOT NULL AND updated_at > $1 ORDER BY updated_at"#,
        since
    )
    .fetch_all(db)
    .await?)
}
