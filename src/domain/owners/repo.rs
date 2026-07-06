use sqlx::PgPool;
use uuid::Uuid;

use super::dto::OwnerRequest;
use super::models::Owner;
use crate::error::AppError;

/// Keyset pagination: rows are ordered by id DESC (UUIDv7 ≈ newest first) and
/// a page starts strictly after the cursor id. `limit + 1` rows are fetched so
/// the caller can tell whether another page exists.
pub async fn list(
    db: &PgPool,
    search: Option<&str>,
    cursor: Option<Uuid>,
    limit: i64,
) -> Result<Vec<Owner>, AppError> {
    let rows = sqlx::query_as!(
        Owner,
        r#"
        SELECT id, name, phone, alt_phone, address, notes, created_at, updated_at
        FROM owners
        WHERE deleted_at IS NULL
          AND ($1::text IS NULL OR name ILIKE '%' || $1 || '%' OR phone ILIKE '%' || $1 || '%')
          AND ($2::uuid IS NULL OR id < $2)
        ORDER BY id DESC
        LIMIT $3
        "#,
        search,
        cursor,
        limit + 1
    )
    .fetch_all(db)
    .await?;
    Ok(rows)
}

pub async fn find(db: &PgPool, id: Uuid) -> Result<Option<Owner>, AppError> {
    let owner = sqlx::query_as!(
        Owner,
        r#"
        SELECT id, name, phone, alt_phone, address, notes, created_at, updated_at
        FROM owners
        WHERE id = $1 AND deleted_at IS NULL
        "#,
        id
    )
    .fetch_optional(db)
    .await?;
    Ok(owner)
}

pub async fn insert(db: &PgPool, id: Uuid, input: &OwnerRequest) -> Result<Owner, AppError> {
    let owner = sqlx::query_as!(
        Owner,
        r#"
        INSERT INTO owners (id, name, phone, alt_phone, address, notes)
        VALUES ($1, $2, $3, $4, $5, $6)
        RETURNING id, name, phone, alt_phone, address, notes, created_at, updated_at
        "#,
        id,
        input.name,
        input.phone,
        input.alt_phone.as_deref(),
        input.address.as_deref(),
        input.notes.as_deref()
    )
    .fetch_one(db)
    .await?;
    Ok(owner)
}

/// Idempotent full-representation upsert (PUT semantics). Re-upserting a
/// soft-deleted row resurrects it — the client is asserting the record exists.
pub async fn upsert(db: &PgPool, id: Uuid, input: &OwnerRequest) -> Result<Owner, AppError> {
    let owner = sqlx::query_as!(
        Owner,
        r#"
        INSERT INTO owners (id, name, phone, alt_phone, address, notes)
        VALUES ($1, $2, $3, $4, $5, $6)
        ON CONFLICT (id) DO UPDATE SET
            name = EXCLUDED.name,
            phone = EXCLUDED.phone,
            alt_phone = EXCLUDED.alt_phone,
            address = EXCLUDED.address,
            notes = EXCLUDED.notes,
            deleted_at = NULL
        RETURNING id, name, phone, alt_phone, address, notes, created_at, updated_at
        "#,
        id,
        input.name,
        input.phone,
        input.alt_phone.as_deref(),
        input.address.as_deref(),
        input.notes.as_deref()
    )
    .fetch_one(db)
    .await?;
    Ok(owner)
}

/// Soft delete; returns false when the row was already gone.
pub async fn soft_delete(db: &PgPool, id: Uuid) -> Result<bool, AppError> {
    let result = sqlx::query!(
        "UPDATE owners SET deleted_at = now() WHERE id = $1 AND deleted_at IS NULL",
        id
    )
    .execute(db)
    .await?;
    Ok(result.rows_affected() > 0)
}
