use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use super::dto::AppointmentRequest;
use super::models::{Appointment, AppointmentStatus};
use crate::error::AppError;

/// Agenda ordering: soonest first (ascending), with a composite
/// `(scheduled_at, id)` keyset cursor.
pub async fn list(
    db: &PgPool,
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
    status: Option<AppointmentStatus>,
    patient_id: Option<Uuid>,
    cursor: Option<Uuid>,
    limit: i64,
) -> Result<Vec<Appointment>, AppError> {
    let rows = sqlx::query_as!(
        Appointment,
        r#"
        SELECT a.id, a.patient_id, p.name AS "patient_name!", o.name AS "owner_name!",
               a.scheduled_at, a.reason, a.status AS "status: AppointmentStatus",
               a.notes, a.created_at, a.updated_at
        FROM appointments a
        JOIN patients p ON p.id = a.patient_id
        JOIN owners o ON o.id = p.owner_id
        WHERE a.deleted_at IS NULL
          AND ($1::timestamptz IS NULL OR a.scheduled_at >= $1)
          AND ($2::timestamptz IS NULL OR a.scheduled_at < $2)
          AND ($3::appointment_status IS NULL OR a.status = $3)
          AND ($4::uuid IS NULL OR a.patient_id = $4)
          AND ($5::uuid IS NULL OR
               (a.scheduled_at, a.id) > (SELECT scheduled_at, id FROM appointments WHERE id = $5))
        ORDER BY a.scheduled_at, a.id
        LIMIT $6
        "#,
        from,
        to,
        status as Option<AppointmentStatus>,
        patient_id,
        cursor,
        limit + 1
    )
    .fetch_all(db)
    .await?;
    Ok(rows)
}

pub async fn find(db: &PgPool, id: Uuid) -> Result<Option<Appointment>, AppError> {
    let appointment = sqlx::query_as!(
        Appointment,
        r#"
        SELECT a.id, a.patient_id, p.name AS "patient_name!", o.name AS "owner_name!",
               a.scheduled_at, a.reason, a.status AS "status: AppointmentStatus",
               a.notes, a.created_at, a.updated_at
        FROM appointments a
        JOIN patients p ON p.id = a.patient_id
        JOIN owners o ON o.id = p.owner_id
        WHERE a.id = $1 AND a.deleted_at IS NULL
        "#,
        id
    )
    .fetch_optional(db)
    .await?;
    Ok(appointment)
}

pub async fn upsert(db: &PgPool, id: Uuid, input: &AppointmentRequest) -> Result<(), AppError> {
    sqlx::query!(
        r#"
        INSERT INTO appointments (id, patient_id, scheduled_at, reason, status, notes)
        VALUES ($1, $2, $3, $4, $5, $6)
        ON CONFLICT (id) DO UPDATE SET
            patient_id = EXCLUDED.patient_id,
            scheduled_at = EXCLUDED.scheduled_at,
            reason = EXCLUDED.reason,
            status = EXCLUDED.status,
            notes = EXCLUDED.notes,
            deleted_at = NULL
        "#,
        id,
        input.patient_id,
        input.scheduled_at,
        input.reason,
        input.status as AppointmentStatus,
        input.notes.as_deref()
    )
    .execute(db)
    .await?;
    Ok(())
}

pub async fn insert(db: &PgPool, id: Uuid, input: &AppointmentRequest) -> Result<(), AppError> {
    sqlx::query!(
        r#"
        INSERT INTO appointments (id, patient_id, scheduled_at, reason, status, notes)
        VALUES ($1, $2, $3, $4, $5, $6)
        "#,
        id,
        input.patient_id,
        input.scheduled_at,
        input.reason,
        input.status as AppointmentStatus,
        input.notes.as_deref()
    )
    .execute(db)
    .await?;
    Ok(())
}

pub async fn soft_delete(db: &PgPool, id: Uuid) -> Result<bool, AppError> {
    let result = sqlx::query!(
        "UPDATE appointments SET deleted_at = now() WHERE id = $1 AND deleted_at IS NULL",
        id
    )
    .execute(db)
    .await?;
    Ok(result.rows_affected() > 0)
}
