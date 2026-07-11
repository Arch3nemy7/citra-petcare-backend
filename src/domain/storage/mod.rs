pub mod dto;
pub mod handlers;
pub mod local;
pub mod s3;

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::config::{Config, StorageConfig};

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("invalid file key: {0}")]
    InvalidKey(String),
    #[error("invalid or expired signature")]
    SignatureInvalid,
    #[error("file not found")]
    NotFound,
    #[error("storage backend error: {0}")]
    Backend(String),
}

/// A presigned request the client performs directly against the storage
/// backend (or, for the local driver, against this API).
#[derive(Debug, Clone)]
pub struct Presigned {
    pub url: String,
    pub method: String,
    /// Headers the client must send verbatim (e.g. Content-Type on upload).
    pub headers: BTreeMap<String, String>,
    pub expires_at: DateTime<Utc>,
}

/// File-storage backend. `async_trait` because native async-fn-in-trait is
/// not yet object-safe, and AppState holds this as `Arc<dyn Storage>`.
#[async_trait]
pub trait Storage: Send + Sync {
    async fn presign_upload(
        &self,
        key: &str,
        content_type: &str,
        ttl: Duration,
    ) -> Result<Presigned, StorageError>;

    async fn presign_download(&self, key: &str, ttl: Duration) -> Result<Presigned, StorageError>;

    /// Driver name for logs and diagnostics.
    fn name(&self) -> &'static str;
}

/// Construct the configured driver at boot.
pub async fn build(config: &Config) -> Result<Arc<dyn Storage>, StorageError> {
    match &config.storage {
        StorageConfig::Local { root } => {
            let storage = local::LocalStorage::new(
                root.clone(),
                config.jwt_secret.clone(),
                config.public_base_url.clone(),
            );
            storage.probe_writable().await?;
            Ok(Arc::new(storage))
        }
        StorageConfig::S3 { .. } => Ok(Arc::new(s3::S3Storage::new(config)?)),
    }
}

/// Storage keys are generated server-side, but validate defensively anyway:
/// no traversal, no absolute paths, conservative character set.
pub fn validate_key(key: &str) -> Result<(), StorageError> {
    if key.is_empty() || key.len() > 512 {
        return Err(StorageError::InvalidKey(
            "key must be 1–512 characters".to_string(),
        ));
    }
    if key.starts_with('/') || key.contains("..") || key.contains('\\') {
        return Err(StorageError::InvalidKey(
            "key must not contain path traversal".to_string(),
        ));
    }
    if !key
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '/'))
    {
        return Err(StorageError::InvalidKey(
            "key may only contain letters, digits, '.', '_', '-' and '/'".to_string(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::validate_key;

    #[test]
    fn accepts_generated_keys() {
        assert!(validate_key("uploads/2026/07/0197xyz-mochi.jpg").is_ok());
    }

    #[test]
    fn rejects_traversal_and_junk() {
        assert!(validate_key("").is_err());
        assert!(validate_key("/etc/passwd").is_err());
        assert!(validate_key("uploads/../secrets").is_err());
        assert!(validate_key("uploads\\windows").is_err());
        assert!(validate_key("uploads/ünïcode.jpg").is_err());
    }
}
