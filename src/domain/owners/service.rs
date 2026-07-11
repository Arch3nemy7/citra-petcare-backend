use sqlx::PgPool;
use uuid::Uuid;

use super::dto::OwnerRequest;
use super::models::Owner;
use super::repo;
use crate::error::AppError;
use crate::http::pagination::Paginated;

pub async fn list(
    db: &PgPool,
    search: Option<&str>,
    cursor: Option<Uuid>,
    limit: i64,
) -> Result<Paginated<Owner>, AppError> {
    let rows = repo::list(db, search, cursor, limit).await?;
    Ok(Paginated::from_rows(rows, limit, |owner| owner.id))
}

pub async fn get(db: &PgPool, id: Uuid) -> Result<Owner, AppError> {
    repo::find(db, id).await?.ok_or(AppError::NotFound("owner"))
}

pub async fn create(db: &PgPool, input: &OwnerRequest) -> Result<Owner, AppError> {
    let id = input.id.unwrap_or_else(Uuid::now_v7);
    repo::insert(db, id, input).await?;
    get(db, id).await
}

pub async fn upsert(db: &PgPool, id: Uuid, input: &OwnerRequest) -> Result<Owner, AppError> {
    repo::upsert(db, id, input).await?;
    get(db, id).await
}

/// Delete an owner, detaching (never deleting) their pets. Returns the number
/// of patients that lost their owner link.
pub async fn delete(db: &PgPool, id: Uuid) -> Result<u64, AppError> {
    repo::soft_delete_detaching_patients(db, id)
        .await?
        .ok_or(AppError::NotFound("owner"))
}
