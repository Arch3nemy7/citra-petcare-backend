//! Local-disk driver for development. It mimics presigned URLs by minting
//! HMAC-signed links back into this API (`/api/v1/storage/local/{key}`),
//! so the Flutter app exercises the exact same upload/download flow as in
//! production against OCI.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use hmac::{Hmac, Mac};
use sha2::Sha256;

use super::{Presigned, Storage, StorageError, validate_key};

type HmacSha256 = Hmac<Sha256>;

pub struct LocalStorage {
    root: PathBuf,
    secret: String,
    base_url: String,
}

impl LocalStorage {
    pub fn new(root: PathBuf, secret: String, base_url: String) -> Self {
        Self {
            root,
            secret,
            base_url,
        }
    }

    fn mac(&self, method: &str, key: &str, expires_unix: i64) -> HmacSha256 {
        let mut mac = HmacSha256::new_from_slice(self.secret.as_bytes())
            .expect("HMAC accepts any key length");
        mac.update(format!("{method}\n{key}\n{expires_unix}").as_bytes());
        mac
    }

    fn sign(&self, method: &str, key: &str, expires_unix: i64) -> String {
        hex::encode(self.mac(method, key, expires_unix).finalize().into_bytes())
    }

    /// Validate an incoming signed request (constant-time via `Mac::verify_slice`).
    pub fn verify(
        &self,
        method: &str,
        key: &str,
        expires_unix: i64,
        signature: &str,
    ) -> Result<(), StorageError> {
        if expires_unix < Utc::now().timestamp() {
            return Err(StorageError::SignatureInvalid);
        }
        let bytes = hex::decode(signature).map_err(|_| StorageError::SignatureInvalid)?;
        self.mac(method, key, expires_unix)
            .verify_slice(&bytes)
            .map_err(|_| StorageError::SignatureInvalid)
    }

    fn signed_url(&self, method: &str, key: &str, ttl: Duration) -> (String, i64) {
        let expires_unix = Utc::now().timestamp() + ttl.as_secs() as i64;
        let signature = self.sign(method, key, expires_unix);
        let url = format!(
            "{}/api/v1/storage/local/{key}?exp={expires_unix}&sig={signature}",
            self.base_url
        );
        (url, expires_unix)
    }

    fn path_for(&self, key: &str) -> Result<PathBuf, StorageError> {
        validate_key(key)?;
        Ok(self.root.join(key))
    }

    pub async fn save(&self, key: &str, bytes: &[u8]) -> Result<(), StorageError> {
        let path = self.path_for(key)?;
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| StorageError::Backend(format!("mkdir failed: {e}")))?;
        }
        tokio::fs::write(&path, bytes)
            .await
            .map_err(|e| StorageError::Backend(format!("write failed: {e}")))
    }

    pub async fn open(&self, key: &str) -> Result<tokio::fs::File, StorageError> {
        let path = self.path_for(key)?;
        tokio::fs::File::open(&path).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                StorageError::NotFound
            } else {
                StorageError::Backend(format!("open failed: {e}"))
            }
        })
    }
}

#[async_trait]
impl Storage for LocalStorage {
    async fn presign_upload(
        &self,
        key: &str,
        content_type: &str,
        ttl: Duration,
    ) -> Result<Presigned, StorageError> {
        validate_key(key)?;
        let (url, expires_unix) = self.signed_url("PUT", key, ttl);
        Ok(Presigned {
            url,
            method: "PUT".to_string(),
            headers: [("content-type".to_string(), content_type.to_string())].into(),
            expires_at: chrono::DateTime::from_timestamp(expires_unix, 0).unwrap_or_else(Utc::now),
        })
    }

    async fn presign_download(&self, key: &str, ttl: Duration) -> Result<Presigned, StorageError> {
        validate_key(key)?;
        let (url, expires_unix) = self.signed_url("GET", key, ttl);
        Ok(Presigned {
            url,
            method: "GET".to_string(),
            headers: BTreeMap::new(),
            expires_at: chrono::DateTime::from_timestamp(expires_unix, 0).unwrap_or_else(Utc::now),
        })
    }

    fn name(&self) -> &'static str {
        "local"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn storage() -> LocalStorage {
        LocalStorage::new(
            PathBuf::from("/tmp/petcare-test"),
            "0123456789abcdef0123456789abcdef".to_string(),
            "http://localhost:8080".to_string(),
        )
    }

    #[test]
    fn sign_verify_roundtrip() {
        let s = storage();
        let exp = Utc::now().timestamp() + 600;
        let sig = s.sign("PUT", "uploads/a.jpg", exp);
        assert!(s.verify("PUT", "uploads/a.jpg", exp, &sig).is_ok());
    }

    #[test]
    fn rejects_tampering_and_expiry() {
        let s = storage();
        let exp = Utc::now().timestamp() + 600;
        let sig = s.sign("PUT", "uploads/a.jpg", exp);
        // wrong method
        assert!(s.verify("GET", "uploads/a.jpg", exp, &sig).is_err());
        // wrong key
        assert!(s.verify("PUT", "uploads/b.jpg", exp, &sig).is_err());
        // shifted expiry
        assert!(s.verify("PUT", "uploads/a.jpg", exp + 1, &sig).is_err());
        // expired
        let past = Utc::now().timestamp() - 10;
        let old_sig = s.sign("PUT", "uploads/a.jpg", past);
        assert!(s.verify("PUT", "uploads/a.jpg", past, &old_sig).is_err());
    }
}
