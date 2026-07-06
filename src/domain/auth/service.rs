use std::sync::LazyLock;

use chrono::{Duration, Utc};
use rand::RngCore;
use sha2::{Digest, Sha256};
use sqlx::PgPool;
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

/// Argon2 hash of a throwaway password, verified when the email is unknown so
/// that "no such user" takes as long as "wrong password" (timing side channel).
static DUMMY_HASH: LazyLock<String> = LazyLock::new(|| {
    use argon2::password_hash::rand_core::OsRng;
    use argon2::password_hash::{PasswordHasher, SaltString};
    argon2::Argon2::default()
        .hash_password(
            b"timing-equalization-dummy",
            &SaltString::generate(&mut OsRng),
        )
        .expect("static argon2 input always hashes")
        .to_string()
});

pub async fn login(
    db: &PgPool,
    config: &Config,
    email: &str,
    pass: &str,
) -> Result<TokenPair, AppError> {
    let user = users::repo::find_by_email(db, email).await?;
    let (hash, user) = match user {
        Some(user) => (user.password_hash.clone(), Some(user)),
        None => (DUMMY_HASH.clone(), None),
    };
    let verified = password::verify_password(hash, pass.to_string()).await?;
    match (user, verified) {
        (Some(user), true) => issue_pair(db, config, user).await,
        _ => Err(AuthError::InvalidCredentials.into()),
    }
}

/// Rotate a refresh token: the presented token is revoked and a new pair is
/// issued. Presenting an already-revoked token is treated as theft (someone
/// replayed a rotated token) and kills every session of that user.
pub async fn refresh(db: &PgPool, config: &Config, raw_token: &str) -> Result<TokenPair, AppError> {
    let token_hash = hash_refresh_token(raw_token);
    let Some(row) = repo::find_by_hash(db, &token_hash).await? else {
        return Err(AuthError::InvalidRefreshToken.into());
    };
    if row.revoked_at.is_some() {
        let revoked = repo::revoke_all_for_user(db, row.user_id).await?;
        tracing::warn!(user_id = %row.user_id, revoked, "refresh token reuse detected; all sessions revoked");
        return Err(AuthError::InvalidRefreshToken.into());
    }
    if row.expires_at <= Utc::now() {
        return Err(AuthError::InvalidRefreshToken.into());
    }
    let Some(user) = users::repo::find_by_id(db, row.user_id).await? else {
        return Err(AuthError::InvalidRefreshToken.into());
    };
    repo::revoke(db, row.id).await?;
    issue_pair(db, config, user).await
}

/// Revoke one refresh token. Idempotent: unknown or foreign tokens are
/// silently ignored so logout never fails.
pub async fn logout(db: &PgPool, user_id: Uuid, raw_token: &str) -> Result<(), AppError> {
    let token_hash = hash_refresh_token(raw_token);
    if let Some(row) = repo::find_by_hash(db, &token_hash).await?
        && row.user_id == user_id
    {
        repo::revoke(db, row.id).await?;
    }
    Ok(())
}

async fn issue_pair(db: &PgPool, config: &Config, user: User) -> Result<TokenPair, AppError> {
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

pub fn hash_refresh_token(raw: &str) -> String {
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
