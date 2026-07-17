use sqlx::PgPool;
use uuid::Uuid;

use super::dto::OwnerRequest;
use super::models::Owner;
use crate::error::AppError;

// Every read carries `patient_count` (active pets) — the app's owner list
// shows "N hewan" per row. The correlated subquery hits
// patients_owner_id_idx, so it stays cheap at clinic scale.

/// Keyset pagination: rows are ordered by id DESC (UUIDv7 ≈ newest first) and
/// a page starts strictly after the cursor id. `limit + 1` rows are fetched so
/// the caller can tell whether another page exists.
pub async fn list(
    db: &PgPool,
    search: Option<&str>,
    cursor: Option<Uuid>,
    limit: i64,
) -> Result<Vec<Owner>, AppError> {
    // Escape LIKE metacharacters so `%`/`_`/`\` in the term match literally
    // across owner name and phone rather than acting as wildcards.
    let search = search.map(crate::db::escape_like);
    let rows = sqlx::query_as!(
        Owner,
        r#"
        SELECT o.id, o.name, o.phone, o.alt_phone, o.address, o.notes,
               (SELECT count(*) FROM patients p
                WHERE p.owner_id = o.id AND p.deleted_at IS NULL) AS "patient_count!",
               o.created_at, o.updated_at
        FROM owners o
        WHERE o.deleted_at IS NULL
          AND ($1::text IS NULL OR o.name ILIKE '%' || $1 || '%' OR o.phone ILIKE '%' || $1 || '%')
          AND ($2::uuid IS NULL OR o.id < $2)
        ORDER BY o.id DESC
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
        SELECT o.id, o.name, o.phone, o.alt_phone, o.address, o.notes,
               (SELECT count(*) FROM patients p
                WHERE p.owner_id = o.id AND p.deleted_at IS NULL) AS "patient_count!",
               o.created_at, o.updated_at
        FROM owners o
        WHERE o.id = $1 AND o.deleted_at IS NULL
        "#,
        id
    )
    .fetch_optional(db)
    .await?;
    Ok(owner)
}

pub async fn insert(db: &PgPool, id: Uuid, input: &OwnerRequest) -> Result<(), AppError> {
    sqlx::query!(
        r#"
        INSERT INTO owners (id, name, phone, alt_phone, address, notes)
        VALUES ($1, $2, $3, $4, $5, $6)
        "#,
        id,
        input.name,
        input.phone.as_deref(),
        input.alt_phone.as_deref(),
        input.address.as_deref(),
        input.notes.as_deref()
    )
    .execute(db)
    .await?;
    Ok(())
}

/// Idempotent full-representation upsert (PUT semantics). Re-upserting a
/// soft-deleted row resurrects it — the client is asserting the record exists.
pub async fn upsert(db: &PgPool, id: Uuid, input: &OwnerRequest) -> Result<(), AppError> {
    sqlx::query!(
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
        "#,
        id,
        input.name,
        input.phone.as_deref(),
        input.alt_phone.as_deref(),
        input.address.as_deref(),
        input.notes.as_deref()
    )
    .execute(db)
    .await?;
    Ok(())
}

/// Soft-delete the owner and detach their active pets (owner_id → NULL) in
/// one transaction, so the pets' medical history survives the owner. Returns
/// None when the owner was already gone, otherwise the detached-pet count.
pub async fn soft_delete_detaching_patients(
    db: &PgPool,
    id: Uuid,
) -> Result<Option<u64>, AppError> {
    let mut tx = db.begin().await?;
    let deleted = sqlx::query!(
        "UPDATE owners SET deleted_at = now() WHERE id = $1 AND deleted_at IS NULL",
        id
    )
    .execute(&mut *tx)
    .await?;
    if deleted.rows_affected() == 0 {
        return Ok(None); // dropping tx rolls back
    }
    let detached = sqlx::query!(
        "UPDATE patients SET owner_id = NULL WHERE owner_id = $1 AND deleted_at IS NULL",
        id
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Some(detached.rows_affected()))
}
