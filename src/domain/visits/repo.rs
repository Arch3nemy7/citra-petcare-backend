use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use super::dto::VisitRequest;
use super::models::{AttachmentKind, Visit, VisitAttachment, WeightPoint};
use crate::error::AppError;

/// Visits are ordered by clinical time (visit_date), not record-creation
/// order, so the keyset cursor is the composite `(visit_date, id)` — the
/// subquery looks up the cursor row's position.
pub async fn list(
    db: &PgPool,
    patient_id: Option<Uuid>,
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
    cursor: Option<Uuid>,
    limit: i64,
) -> Result<Vec<Visit>, AppError> {
    let rows = sqlx::query_as!(
        Visit,
        r#"
        SELECT v.id, v.patient_id, p.name AS "patient_name!", v.vet_id,
               u.name AS "vet_name!", v.visit_date, v.complaint, v.temperature_c,
               v.weight_kg, v.exam_notes, v.diagnosis, v.treatment,
               v.prescription, v.follow_up_date, v.created_at, v.updated_at
        FROM visits v
        JOIN patients p ON p.id = v.patient_id
        JOIN users u ON u.id = v.vet_id
        WHERE v.deleted_at IS NULL
          AND ($1::uuid IS NULL OR v.patient_id = $1)
          AND ($2::timestamptz IS NULL OR v.visit_date >= $2)
          AND ($3::timestamptz IS NULL OR v.visit_date < $3)
          AND ($4::uuid IS NULL OR
               (v.visit_date, v.id) < (SELECT visit_date, id FROM visits WHERE id = $4))
        ORDER BY v.visit_date DESC, v.id DESC
        LIMIT $5
        "#,
        patient_id,
        from,
        to,
        cursor,
        limit + 1
    )
    .fetch_all(db)
    .await?;
    Ok(rows)
}

pub async fn find(db: &PgPool, id: Uuid) -> Result<Option<Visit>, AppError> {
    let visit = sqlx::query_as!(
        Visit,
        r#"
        SELECT v.id, v.patient_id, p.name AS "patient_name!", v.vet_id,
               u.name AS "vet_name!", v.visit_date, v.complaint, v.temperature_c,
               v.weight_kg, v.exam_notes, v.diagnosis, v.treatment,
               v.prescription, v.follow_up_date, v.created_at, v.updated_at
        FROM visits v
        JOIN patients p ON p.id = v.patient_id
        JOIN users u ON u.id = v.vet_id
        WHERE v.id = $1 AND v.deleted_at IS NULL
        "#,
        id
    )
    .fetch_optional(db)
    .await?;
    Ok(visit)
}

pub async fn insert(db: &PgPool, id: Uuid, input: &VisitRequest) -> Result<(), AppError> {
    sqlx::query!(
        r#"
        INSERT INTO visits
            (id, patient_id, vet_id, visit_date, complaint, temperature_c, weight_kg,
             exam_notes, diagnosis, treatment, prescription, follow_up_date)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
        "#,
        id,
        input.patient_id,
        input.vet_id,
        input.visit_date,
        input.complaint,
        input.temperature_c,
        input.weight_kg,
        input.exam_notes.as_deref(),
        input.diagnosis.as_deref(),
        input.treatment.as_deref(),
        input.prescription.as_deref(),
        input.follow_up_date
    )
    .execute(db)
    .await?;
    Ok(())
}

pub async fn upsert(db: &PgPool, id: Uuid, input: &VisitRequest) -> Result<(), AppError> {
    sqlx::query!(
        r#"
        INSERT INTO visits
            (id, patient_id, vet_id, visit_date, complaint, temperature_c, weight_kg,
             exam_notes, diagnosis, treatment, prescription, follow_up_date)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
        ON CONFLICT (id) DO UPDATE SET
            patient_id = EXCLUDED.patient_id,
            vet_id = EXCLUDED.vet_id,
            visit_date = EXCLUDED.visit_date,
            complaint = EXCLUDED.complaint,
            temperature_c = EXCLUDED.temperature_c,
            weight_kg = EXCLUDED.weight_kg,
            exam_notes = EXCLUDED.exam_notes,
            diagnosis = EXCLUDED.diagnosis,
            treatment = EXCLUDED.treatment,
            prescription = EXCLUDED.prescription,
            follow_up_date = EXCLUDED.follow_up_date,
            deleted_at = NULL
        "#,
        id,
        input.patient_id,
        input.vet_id,
        input.visit_date,
        input.complaint,
        input.temperature_c,
        input.weight_kg,
        input.exam_notes.as_deref(),
        input.diagnosis.as_deref(),
        input.treatment.as_deref(),
        input.prescription.as_deref(),
        input.follow_up_date
    )
    .execute(db)
    .await?;
    Ok(())
}

/// Soft-delete a visit and its attachments in one transaction so sync clients
/// receive tombstones for both.
pub async fn soft_delete_with_attachments(db: &PgPool, id: Uuid) -> Result<bool, AppError> {
    let mut tx = db.begin().await?;
    let result = sqlx::query!(
        "UPDATE visits SET deleted_at = now() WHERE id = $1 AND deleted_at IS NULL",
        id
    )
    .execute(&mut *tx)
    .await?;
    if result.rows_affected() == 0 {
        return Ok(false); // dropping tx rolls back
    }
    sqlx::query!(
        "UPDATE visit_attachments SET deleted_at = now() WHERE visit_id = $1 AND deleted_at IS NULL",
        id
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(true)
}

pub async fn attachments_for(
    db: &PgPool,
    visit_id: Uuid,
) -> Result<Vec<VisitAttachment>, AppError> {
    let rows = sqlx::query_as!(
        VisitAttachment,
        r#"
        SELECT id, visit_id, file_key, kind AS "kind: AttachmentKind", created_at, updated_at
        FROM visit_attachments
        WHERE visit_id = $1 AND deleted_at IS NULL
        ORDER BY created_at
        "#,
        visit_id
    )
    .fetch_all(db)
    .await?;
    Ok(rows)
}

pub async fn insert_attachment(
    db: &PgPool,
    id: Uuid,
    visit_id: Uuid,
    file_key: &str,
    kind: AttachmentKind,
) -> Result<VisitAttachment, AppError> {
    let attachment = sqlx::query_as!(
        VisitAttachment,
        r#"
        INSERT INTO visit_attachments (id, visit_id, file_key, kind)
        VALUES ($1, $2, $3, $4)
        RETURNING id, visit_id, file_key, kind AS "kind: AttachmentKind", created_at, updated_at
        "#,
        id,
        visit_id,
        file_key,
        kind as AttachmentKind
    )
    .fetch_one(db)
    .await?;
    Ok(attachment)
}

pub async fn soft_delete_attachment(
    db: &PgPool,
    visit_id: Uuid,
    attachment_id: Uuid,
) -> Result<bool, AppError> {
    let result = sqlx::query!(
        r#"
        UPDATE visit_attachments SET deleted_at = now()
        WHERE id = $1 AND visit_id = $2 AND deleted_at IS NULL
        "#,
        attachment_id,
        visit_id
    )
    .execute(db)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// Chronological weight series for one patient, from visit measurements.
pub async fn weight_history(db: &PgPool, patient_id: Uuid) -> Result<Vec<WeightPoint>, AppError> {
    let rows = sqlx::query_as!(
        WeightPoint,
        r#"
        SELECT id AS visit_id, visit_date, weight_kg AS "weight_kg!"
        FROM visits
        WHERE patient_id = $1 AND weight_kg IS NOT NULL AND deleted_at IS NULL
        ORDER BY visit_date
        "#,
        patient_id
    )
    .fetch_all(db)
    .await?;
    Ok(rows)
}
