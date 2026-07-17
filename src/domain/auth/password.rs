//! Argon2id password hashing with the crate's OWASP-recommended defaults
//! (m=19456 KiB, t=2, p=1).
//!
//! Hashing/verification burns tens of milliseconds of CPU, so both helpers
//! run on tokio's blocking thread pool — an `.await` on the async runtime
//! must never block a worker thread for that long.

use argon2::Argon2;
use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};

use crate::error::AppError;

pub async fn hash_password(password: String) -> Result<String, AppError> {
    tokio::task::spawn_blocking(move || {
        let salt = SaltString::generate(&mut OsRng);
        Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .map(|hash| hash.to_string())
            .map_err(|e| AppError::Internal(format!("password hashing failed: {e}")))
    })
    .await
    .map_err(|e| AppError::Internal(format!("hashing task panicked: {e}")))?
}

/// Returns `Ok(false)` (not an error) for wrong passwords *and* unparseable
/// hashes, so callers can't accidentally turn a bad row into a 500 that leaks
/// which accounts exist.
pub async fn verify_password(password_hash: String, password: String) -> Result<bool, AppError> {
    tokio::task::spawn_blocking(move || {
        let Ok(parsed) = PasswordHash::new(&password_hash) else {
            tracing::warn!("stored password hash could not be parsed");
            return Ok(false);
        };
        Ok(Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok())
    })
    .await
    .map_err(|e| AppError::Internal(format!("verification task panicked: {e}")))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn hash_and_verify_roundtrip() {
        let hash = hash_password("s3cret-password".to_string()).await.unwrap();
        assert!(hash.starts_with("$argon2id$"));
        assert!(
            verify_password(hash.clone(), "s3cret-password".to_string())
                .await
                .unwrap()
        );
        assert!(
            !verify_password(hash, "wrong-password".to_string())
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn garbage_hash_is_not_an_error() {
        assert!(
            !verify_password("not-a-hash".to_string(), "pw".to_string())
                .await
                .unwrap()
        );
    }
}
