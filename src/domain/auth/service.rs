use chrono::{Duration, Utc};
use rand::RngCore;
use sha2::{Digest, Sha256};
use sqlx::{PgExecutor, PgPool};
use uuid::Uuid;

use super::{AuthError, jwt, password, repo};
use crate::config::Config;
use crate::domain::users;
use crate::domain::users::models::User;
use crate::error::AppError;

/// Result of a successful login/refresh. The raw refresh token exists only
/// here and in the HTTP response — the database stores its SHA-256 hash.
#[derive(Debug)]
pub struct TokenPair {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: i64,
    pub user: User,
}

pub async fn login(
    db: &PgPool,
    config: &Config,
    email: &str,
    pass: &str,
) -> Result<TokenPair, AppError> {
    let user = users::repo::find_by_email(db, email).await?;
    // Verify against a fixed reference hash when the email is unknown, so the
    // lookup costs the same as a real wrong-password verify (timing side channel).
    let hash = match &user {
        Some(user) => user.password_hash.clone(),
        None => password::reference_hash(),
    };
    let verified = password::verify_password(hash, pass.to_string()).await?;
    match user {
        Some(user) if verified => issue_pair(db, config, user).await,
        _ => Err(AuthError::InvalidCredentials.into()),
    }
}

/// Rotate a refresh token: the presented token is revoked and a new pair is
/// issued. Presenting an already-revoked token is treated as theft (someone
/// replayed a rotated token) and kills every session of that user.
///
/// Claim and replacement run in one transaction: the claim is an atomic
/// conditional UPDATE, so of any concurrent presentations exactly one wins
/// and the losers hit the theft path — and if signing or the insert fails
/// before commit, the claim rolls back and the client's token stays usable
/// (a transient error must not burn the only token the client has).
pub async fn refresh(db: &PgPool, config: &Config, raw_token: &str) -> Result<TokenPair, AppError> {
    let token_hash = hash_refresh_token(raw_token);
    let mut tx = db.begin().await?;
    let Some(claimed) = repo::claim_by_hash(&mut *tx, &token_hash).await? else {
        drop(tx);
        // Unknown hash → plain 401. Known but already revoked → reuse.
        if let Some(row) = repo::find_by_hash(db, &token_hash).await?
            && row.revoked_at.is_some()
        {
            let revoked = repo::revoke_all_for_user(db, row.user_id).await?;
            tracing::warn!(user_id = %row.user_id, revoked, "refresh token reuse detected; all sessions revoked");
        }
        return Err(AuthError::InvalidRefreshToken.into());
    };
    if claimed.expires_at <= Utc::now() {
        // Expired: drop the transaction so the claim rolls back — replaying
        // an expired token stays a plain 401 rather than tripping theft
        // detection, matching the pre-claim behavior.
        return Err(AuthError::InvalidRefreshToken.into());
    }
    let Some(user) = users::repo::find_by_id(&mut *tx, claimed.user_id).await? else {
        return Err(AuthError::InvalidRefreshToken.into());
    };
    let pair = issue_pair(&mut *tx, config, user).await?;
    tx.commit().await?;
    Ok(pair)
}

/// Revoke one refresh token. Possession of the (256-bit, unguessable) token
/// is the proof of ownership — the same trust model `refresh` uses — so no
/// access token is required and an idle device can always end its session.
/// Idempotent: unknown or already-revoked tokens are silently ignored so
/// logout never fails.
pub async fn logout(db: &PgPool, raw_token: &str) -> Result<(), AppError> {
    repo::revoke_by_hash(db, &hash_refresh_token(raw_token)).await
}

async fn issue_pair(
    db: impl PgExecutor<'_>,
    config: &Config,
    user: User,
) -> Result<TokenPair, AppError> {
    let access_token = jwt::encode_access_token(&user, config)?;
    let (raw_refresh, refresh_hash) = generate_refresh_token();
    let expires_at = Utc::now() + Duration::days(config.refresh_token_ttl_days);
    repo::insert(db, Uuid::now_v7(), user.id, &refresh_hash, expires_at).await?;
    Ok(TokenPair {
        access_token,
        refresh_token: raw_refresh,
        expires_in: config.access_token_ttl_secs,
        user,
    })
}

/// 256-bit random token, hex-encoded (64 chars), plus its storage hash.
fn generate_refresh_token() -> (String, String) {
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    let raw = hex::encode(bytes);
    let hash = hash_refresh_token(&raw);
    (raw, hash)
}

fn hash_refresh_token(raw: &str) -> String {
    hex::encode(Sha256::digest(raw.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refresh_tokens_are_unique_and_hashed() {
        let (raw_a, hash_a) = generate_refresh_token();
        let (raw_b, hash_b) = generate_refresh_token();
        assert_eq!(raw_a.len(), 64);
        assert_ne!(raw_a, raw_b);
        assert_ne!(hash_a, hash_b);
        assert_eq!(hash_a, hash_refresh_token(&raw_a));
        assert_ne!(raw_a, hash_a);
    }
}
