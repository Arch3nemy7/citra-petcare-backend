use sqlx::{PgExecutor, PgPool};
use uuid::Uuid;

use super::models::{User, UserRole};
use crate::error::AppError;

pub async fn find_by_id(db: impl PgExecutor<'_>, id: Uuid) -> Result<Option<User>, AppError> {
    let user = sqlx::query_as!(
        User,
        r#"
        SELECT id, name, email, password_hash, role AS "role: UserRole", created_at, updated_at
        FROM users
        WHERE id = $1 AND deleted_at IS NULL
        "#,
        id
    )
    .fetch_optional(db)
    .await?;
    Ok(user)
}

pub async fn find_by_email(db: &PgPool, email: &str) -> Result<Option<User>, AppError> {
    let user = sqlx::query_as!(
        User,
        r#"
        SELECT id, name, email, password_hash, role AS "role: UserRole", created_at, updated_at
        FROM users
        WHERE lower(email) = lower($1) AND deleted_at IS NULL
        "#,
        email
    )
    .fetch_optional(db)
    .await?;
    Ok(user)
}

pub async fn list(db: &PgPool) -> Result<Vec<User>, AppError> {
    let users = sqlx::query_as!(
        User,
        r#"
        SELECT id, name, email, password_hash, role AS "role: UserRole", created_at, updated_at
        FROM users
        WHERE deleted_at IS NULL
        ORDER BY name
        "#
    )
    .fetch_all(db)
    .await?;
    Ok(users)
}
