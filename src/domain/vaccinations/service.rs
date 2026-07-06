use sqlx::PgPool;
use uuid::Uuid;

use super::dto::{CreateVaccinationRequest, UpsertVaccinationRequest};
use super::models::Vaccination;
use super::repo;
use crate::domain::patients;
use crate::error::AppError;
use crate::http::pagination::Paginated;

pub async fn list_for_patient(
    db: &PgPool,
    patient_id: Uuid,
    cursor: Option<Uuid>,
    limit: i64,
) -> Result<Paginated<Vaccination>, AppError> {
    patients::service::get(db, patient_id).await?; // 404 for unknown patients
    let rows = repo::list_for_patient(db, patient_id, cursor, limit).await?;
    Ok(Paginated::from_rows(rows, limit, |v| v.id))
}

pub async fn get(db: &PgPool, id: Uuid) -> Result<Vaccination, AppError> {
    repo::find(db, id)
        .await?
        .ok_or(AppError::NotFound("vaccination"))
}

pub async fn create_for_patient(
    db: &PgPool,
    patient_id: Uuid,
    input: &CreateVaccinationRequest,
) -> Result<Vaccination, AppError> {
    patients::service::get(db, patient_id).await?;
    let id = input.id.unwrap_or_else(Uuid::now_v7);
    repo::upsert(
        db,
        id,
        patient_id,
        input.visit_id,
        &input.vaccine_name,
        input.date_given,
        input.batch_no.as_deref(),
        input.next_due_date,
    )
    .await?;
    get(db, id).await
}

pub async fn upsert(
    db: &PgPool,
    id: Uuid,
    input: &UpsertVaccinationRequest,
) -> Result<Vaccination, AppError> {
    repo::upsert(
        db,
        id,
        input.patient_id,
        input.visit_id,
        &input.vaccine_name,
        input.date_given,
        input.batch_no.as_deref(),
        input.next_due_date,
    )
    .await?;
    get(db, id).await
}

pub async fn delete(db: &PgPool, id: Uuid) -> Result<(), AppError> {
    if !repo::soft_delete(db, id).await? {
        return Err(AppError::NotFound("vaccination"));
    }
    Ok(())
}
