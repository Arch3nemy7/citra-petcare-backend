use chrono::NaiveDate;
use sqlx::PgPool;
use uuid::Uuid;

use super::models::Vaccination;
use crate::error::AppError;

/// Ordered by administration date (newest first) with a composite keyset
/// cursor, same pattern as visits.
pub async fn list_for_patient(
    db: &PgPool,
    patient_id: Uuid,
    cursor: Option<Uuid>,
    limit: i64,
) -> Result<Vec<Vaccination>, AppError> {
    let rows = sqlx::query_as!(
        Vaccination,
        r#"
        SELECT v.id, v.patient_id, p.name AS "patient_name!", v.visit_id,
               v.vaccine_name, v.date_given, v.batch_no, v.next_due_date,
               v.created_at, v.updated_at
        FROM vaccinations v
        JOIN patients p ON p.id = v.patient_id
        WHERE v.deleted_at IS NULL
          AND v.patient_id = $1
          AND ($2::uuid IS NULL OR
               (v.date_given, v.id) < (SELECT date_given, id FROM vaccinations WHERE id = $2))
        ORDER BY v.date_given DESC, v.id DESC
        LIMIT $3
        "#,
        patient_id,
        cursor,
        limit + 1
    )
    .fetch_all(db)
    .await?;
    Ok(rows)
}

pub async fn find(db: &PgPool, id: Uuid) -> Result<Option<Vaccination>, AppError> {
    let vaccination = sqlx::query_as!(
        Vaccination,
        r#"
        SELECT v.id, v.patient_id, p.name AS "patient_name!", v.visit_id,
               v.vaccine_name, v.date_given, v.batch_no, v.next_due_date,
               v.created_at, v.updated_at
        FROM vaccinations v
        JOIN patients p ON p.id = v.patient_id
        WHERE v.id = $1 AND v.deleted_at IS NULL
        "#,
        id
    )
    .fetch_optional(db)
    .await?;
    Ok(vaccination)
}

#[allow(clippy::too_many_arguments)] // flat argument list keeps the query obvious
pub async fn upsert(
    db: &PgPool,
    id: Uuid,
    patient_id: Uuid,
    visit_id: Option<Uuid>,
    vaccine_name: &str,
    date_given: NaiveDate,
    batch_no: Option<&str>,
    next_due_date: Option<NaiveDate>,
) -> Result<(), AppError> {
    sqlx::query!(
        r#"
        INSERT INTO vaccinations
            (id, patient_id, visit_id, vaccine_name, date_given, batch_no, next_due_date)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        ON CONFLICT (id) DO UPDATE SET
            patient_id = EXCLUDED.patient_id,
            visit_id = EXCLUDED.visit_id,
            vaccine_name = EXCLUDED.vaccine_name,
            date_given = EXCLUDED.date_given,
            batch_no = EXCLUDED.batch_no,
            next_due_date = EXCLUDED.next_due_date,
            deleted_at = NULL
        "#,
        id,
        patient_id,
        visit_id,
        vaccine_name,
        date_given,
        batch_no,
        next_due_date
    )
    .execute(db)
    .await?;
    Ok(())
}

pub async fn soft_delete(db: &PgPool, id: Uuid) -> Result<bool, AppError> {
    let result = sqlx::query!(
        "UPDATE vaccinations SET deleted_at = now() WHERE id = $1 AND deleted_at IS NULL",
        id
    )
    .execute(db)
    .await?;
    Ok(result.rows_affected() > 0)
}
