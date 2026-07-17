use chrono::{DateTime, Utc};
use sqlx::{PgExecutor, PgPool};
use uuid::Uuid;

use crate::error::AppError;

/// Row from `refresh_tokens`, minus the hash (already known to the caller).
#[derive(Debug)]
pub struct RefreshTokenRow {
    pub user_id: Uuid,
    pub expires_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
}

/// Result of successfully claiming a token via [`claim_by_hash`].
#[derive(Debug)]
pub struct ClaimedToken {
    pub user_id: Uuid,
    pub expires_at: DateTime<Utc>,
}

pub async fn insert(
    db: impl PgExecutor<'_>,
    id: Uuid,
    user_id: Uuid,
    token_hash: &str,
    expires_at: DateTime<Utc>,
) -> Result<(), AppError> {
    sqlx::query!(
        "INSERT INTO refresh_tokens (id, user_id, token_hash, expires_at) VALUES ($1, $2, $3, $4)",
        id,
        user_id,
        token_hash,
        expires_at
    )
    .execute(db)
    .await?;
    Ok(())
}

pub async fn find_by_hash(
    db: &PgPool,
    token_hash: &str,
) -> Result<Option<RefreshTokenRow>, AppError> {
    let row = sqlx::query_as!(
        RefreshTokenRow,
        "SELECT user_id, expires_at, revoked_at FROM refresh_tokens WHERE token_hash = $1",
        token_hash
    )
    .fetch_optional(db)
    .await?;
    Ok(row)
}

/// Atomically consume a live token: set `revoked_at` and return the owner and
/// expiry. The conditional UPDATE guarantees that of any concurrent
/// presentations of the same token exactly one gets `Some` — the losers see
/// `None`, which callers treat as the reuse/theft signal.
pub async fn claim_by_hash(
    db: impl PgExecutor<'_>,
    token_hash: &str,
) -> Result<Option<ClaimedToken>, AppError> {
    let row = sqlx::query_as!(
        ClaimedToken,
        "UPDATE refresh_tokens SET revoked_at = now() WHERE token_hash = $1 AND revoked_at IS NULL RETURNING user_id, expires_at",
        token_hash
    )
    .fetch_optional(db)
    .await?;
    Ok(row)
}

/// Revoke whatever live token matches the hash. Idempotent by construction:
/// unknown or already-revoked tokens match zero rows.
pub async fn revoke_by_hash(db: &PgPool, token_hash: &str) -> Result<(), AppError> {
    sqlx::query!(
        "UPDATE refresh_tokens SET revoked_at = now() WHERE token_hash = $1 AND revoked_at IS NULL",
        token_hash
    )
    .execute(db)
    .await?;
    Ok(())
}

/// Revoke every live token of a user — used when refresh-token reuse is
/// detected (assume the token family is compromised) .
pub async fn revoke_all_for_user(db: &PgPool, user_id: Uuid) -> Result<u64, AppError> {
    let result = sqlx::query!(
        "UPDATE refresh_tokens SET revoked_at = now() WHERE user_id = $1 AND revoked_at IS NULL",
        user_id
    )
    .execute(db)
    .await?;
    Ok(result.rows_affected())
}

/// Housekeeping: drop rows that expired more than 30 days ago (called by the
/// daily scheduler job). Hard DELETE is fine — these are not domain data.
pub async fn prune_expired(db: &PgPool) -> Result<u64, AppError> {
    let result =
        sqlx::query!("DELETE FROM refresh_tokens WHERE expires_at < now() - interval '30 days'")
            .execute(db)
            .await?;
    Ok(result.rows_affected())
}
