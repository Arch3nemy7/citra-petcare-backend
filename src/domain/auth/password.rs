//! Argon2id password hashing with the crate's OWASP-recommended defaults
//! (m=19456 KiB, t=2, p=1).
//!
//! Hashing/verification burns tens of milliseconds of CPU, so both helpers
//! run on tokio's blocking thread pool — an `.await` on the async runtime
//! must never block a worker thread for that long.

use std::sync::LazyLock;

use argon2::Argon2;
use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};

use crate::error::AppError;

/// Argon2 hash of a throwaway password, verified when an email is unknown so
/// that "no such user" takes as long as "wrong password" (timing side channel).
/// Hashed with the same `Argon2::default()` parameters as real passwords, so if
/// those parameters ever change this dummy tracks them and the two paths stay
/// indistinguishable in cost.
static REFERENCE_HASH: LazyLock<String> = LazyLock::new(|| {
    Argon2::default()
        .hash_password(
            b"timing-equalization-dummy",
            &SaltString::generate(&mut OsRng),
        )
        .expect("static argon2 input always hashes")
        .to_string()
});

/// A stable Argon2 hash to verify against on the unknown-email login path, so
/// that lookup costs the same as a real wrong-password verify.
pub fn reference_hash() -> String {
    REFERENCE_HASH.clone()
}

/// Force [`REFERENCE_HASH`] eagerly. Its initializer burns tens of milliseconds
/// of Argon2 CPU; called from a blocking context at boot so the first
/// unknown-email login never runs it on an async worker thread.
pub fn warm_up() {
    LazyLock::force(&REFERENCE_HASH);
}

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
