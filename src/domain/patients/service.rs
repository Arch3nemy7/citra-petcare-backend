use sqlx::PgPool;
use uuid::Uuid;

use super::dto::PatientRequest;
use super::models::Patient;
use super::repo;
use crate::domain::visits;
use crate::domain::visits::models::WeightPoint;
use crate::error::AppError;
use crate::http::pagination::Paginated;

pub async fn list(
    db: &PgPool,
    search: Option<&str>,
    owner_id: Option<Uuid>,
    cursor: Option<Uuid>,
    limit: i64,
) -> Result<Paginated<Patient>, AppError> {
    let rows = repo::list(db, search, owner_id, cursor, limit).await?;
    Ok(Paginated::from_rows(rows, limit, |patient| patient.id))
}

pub async fn get(db: &PgPool, id: Uuid) -> Result<Patient, AppError> {
    repo::find(db, id)
        .await?
        .ok_or(AppError::NotFound("patient"))
}

pub async fn create(db: &PgPool, input: &PatientRequest) -> Result<Patient, AppError> {
    let id = input.id.unwrap_or_else(Uuid::now_v7);
    repo::insert(db, id, input).await?;
    // re-read to pick up server-side values (timestamps, joined owner name)
    get(db, id).await
}

pub async fn upsert(db: &PgPool, id: Uuid, input: &PatientRequest) -> Result<Patient, AppError> {
    repo::upsert(db, id, input).await?;
    get(db, id).await
}

pub async fn delete(db: &PgPool, id: Uuid) -> Result<(), AppError> {
    if !repo::soft_delete(db, id).await? {
        return Err(AppError::NotFound("patient"));
    }
    Ok(())
}

/// Weight-over-time series, derived from `visits.weight_kg`.
pub async fn weight_history(db: &PgPool, patient_id: Uuid) -> Result<Vec<WeightPoint>, AppError> {
    // 404 for unknown patients instead of a silently-empty series
    get(db, patient_id).await?;
    visits::repo::weight_history(db, patient_id).await
}
