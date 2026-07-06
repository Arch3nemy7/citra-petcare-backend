use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use super::dto::AppointmentRequest;
use super::models::{Appointment, AppointmentStatus};
use super::repo;
use crate::error::AppError;
use crate::http::pagination::Paginated;

#[allow(clippy::too_many_arguments)] // thin pass-through of the query params
pub async fn list(
    db: &PgPool,
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
    status: Option<AppointmentStatus>,
    patient_id: Option<Uuid>,
    cursor: Option<Uuid>,
    limit: i64,
) -> Result<Paginated<Appointment>, AppError> {
    let rows = repo::list(db, from, to, status, patient_id, cursor, limit).await?;
    Ok(Paginated::from_rows(rows, limit, |a| a.id))
}

pub async fn get(db: &PgPool, id: Uuid) -> Result<Appointment, AppError> {
    repo::find(db, id)
        .await?
        .ok_or(AppError::NotFound("appointment"))
}

pub async fn create(db: &PgPool, input: &AppointmentRequest) -> Result<Appointment, AppError> {
    let id = input.id.unwrap_or_else(Uuid::now_v7);
    repo::insert(db, id, input).await?;
    get(db, id).await
}

pub async fn upsert(
    db: &PgPool,
    id: Uuid,
    input: &AppointmentRequest,
) -> Result<Appointment, AppError> {
    repo::upsert(db, id, input).await?;
    get(db, id).await
}

pub async fn delete(db: &PgPool, id: Uuid) -> Result<(), AppError> {
    if !repo::soft_delete(db, id).await? {
        return Err(AppError::NotFound("appointment"));
    }
    Ok(())
}
