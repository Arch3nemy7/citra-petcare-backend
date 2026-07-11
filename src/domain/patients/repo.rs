use sqlx::PgPool;
use uuid::Uuid;

use super::dto::PatientRequest;
use super::models::{Patient, PatientStatus, Sex, Species};
use crate::error::AppError;

// Every read LEFT JOINs owners for the denormalized `owner_name` — the join
// column is nullable because pets may be detached ("Tanpa pemilik"). The `?`
// marker tells sqlx the joined name may be NULL even though owners.name isn't.

pub async fn list(
    db: &PgPool,
    search: Option<&str>,
    owner_id: Option<Uuid>,
    cursor: Option<Uuid>,
    limit: i64,
) -> Result<Vec<Patient>, AppError> {
    let rows = sqlx::query_as!(
        Patient,
        r#"
        SELECT p.id, p.owner_id, o.name AS "owner_name?", p.name,
               p.species AS "species: Species", p.breed, p.sex AS "sex: Sex",
               p.sterilized, p.birth_date, p.color_markings, p.microchip_no,
               p.photo_key, p.allergies, p.alert_notes,
               p.status AS "status: PatientStatus", p.created_at, p.updated_at
        FROM patients p
        LEFT JOIN owners o ON o.id = p.owner_id
        WHERE p.deleted_at IS NULL
          AND ($1::text IS NULL
               OR p.name ILIKE '%' || $1 || '%'
               OR o.name ILIKE '%' || $1 || '%'
               OR o.phone ILIKE '%' || $1 || '%')
          AND ($2::uuid IS NULL OR p.owner_id = $2)
          AND ($3::uuid IS NULL OR p.id < $3)
        ORDER BY p.id DESC
        LIMIT $4
        "#,
        search,
        owner_id,
        cursor,
        limit + 1
    )
    .fetch_all(db)
    .await?;
    Ok(rows)
}

pub async fn find(db: &PgPool, id: Uuid) -> Result<Option<Patient>, AppError> {
    let patient = sqlx::query_as!(
        Patient,
        r#"
        SELECT p.id, p.owner_id, o.name AS "owner_name?", p.name,
               p.species AS "species: Species", p.breed, p.sex AS "sex: Sex",
               p.sterilized, p.birth_date, p.color_markings, p.microchip_no,
               p.photo_key, p.allergies, p.alert_notes,
               p.status AS "status: PatientStatus", p.created_at, p.updated_at
        FROM patients p
        LEFT JOIN owners o ON o.id = p.owner_id
        WHERE p.id = $1 AND p.deleted_at IS NULL
        "#,
        id
    )
    .fetch_optional(db)
    .await?;
    Ok(patient)
}

pub async fn insert(db: &PgPool, id: Uuid, input: &PatientRequest) -> Result<(), AppError> {
    sqlx::query!(
        r#"
        INSERT INTO patients
            (id, owner_id, name, species, breed, sex, sterilized, birth_date,
             color_markings, microchip_no, photo_key, allergies, alert_notes, status)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
        "#,
        id,
        input.owner_id,
        input.name,
        input.species as Species,
        input.breed.as_deref(),
        input.sex as Sex,
        input.sterilized,
        input.birth_date,
        input.color_markings.as_deref(),
        input.microchip_no.as_deref(),
        input.photo_key.as_deref(),
        input.allergies.as_deref(),
        input.alert_notes.as_deref(),
        input.status as PatientStatus
    )
    .execute(db)
    .await?;
    Ok(())
}

/// Idempotent full-representation upsert; resurrects a soft-deleted row.
pub async fn upsert(db: &PgPool, id: Uuid, input: &PatientRequest) -> Result<(), AppError> {
    sqlx::query!(
        r#"
        INSERT INTO patients
            (id, owner_id, name, species, breed, sex, sterilized, birth_date,
             color_markings, microchip_no, photo_key, allergies, alert_notes, status)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
        ON CONFLICT (id) DO UPDATE SET
            owner_id = EXCLUDED.owner_id,
            name = EXCLUDED.name,
            species = EXCLUDED.species,
            breed = EXCLUDED.breed,
            sex = EXCLUDED.sex,
            sterilized = EXCLUDED.sterilized,
            birth_date = EXCLUDED.birth_date,
            color_markings = EXCLUDED.color_markings,
            microchip_no = EXCLUDED.microchip_no,
            photo_key = EXCLUDED.photo_key,
            allergies = EXCLUDED.allergies,
            alert_notes = EXCLUDED.alert_notes,
            status = EXCLUDED.status,
            deleted_at = NULL
        "#,
        id,
        input.owner_id,
        input.name,
        input.species as Species,
        input.breed.as_deref(),
        input.sex as Sex,
        input.sterilized,
        input.birth_date,
        input.color_markings.as_deref(),
        input.microchip_no.as_deref(),
        input.photo_key.as_deref(),
        input.allergies.as_deref(),
        input.alert_notes.as_deref(),
        input.status as PatientStatus
    )
    .execute(db)
    .await?;
    Ok(())
}

pub async fn soft_delete(db: &PgPool, id: Uuid) -> Result<bool, AppError> {
    let result = sqlx::query!(
        "UPDATE patients SET deleted_at = now() WHERE id = $1 AND deleted_at IS NULL",
        id
    )
    .execute(db)
    .await?;
    Ok(result.rows_affected() > 0)
}
