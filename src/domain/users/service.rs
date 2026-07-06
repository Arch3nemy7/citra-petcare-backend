use sqlx::PgPool;
use uuid::Uuid;

use super::models::User;
use super::repo;
use crate::error::AppError;

pub async fn list_users(db: &PgPool) -> Result<Vec<User>, AppError> {
    repo::list(db).await
}

pub async fn get_user(db: &PgPool, id: Uuid) -> Result<User, AppError> {
    repo::find_by_id(db, id)
        .await?
        .ok_or(AppError::NotFound("user"))
}
