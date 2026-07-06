//! S3-compatible driver targeting Oracle Cloud Object Storage's S3
//! Compatibility API. All client traffic uses presigned URLs, so the API
//! server never proxies file bytes.
//!
//! OCI specifics baked in below:
//! - endpoint `https://{namespace}.compat.objectstorage.{region}.oraclecloud.com`
//! - path-style addressing (virtual-hosted style is not supported)
//! - checksum calculation "when required" (OCI rejects the aws-chunked
//!   content encoding the SDK would otherwise use)
//! - credentials are OCI *Customer Secret Keys*, passed via env

use std::time::Duration;

use async_trait::async_trait;
use aws_sdk_s3::config::{
    BehaviorVersion, Credentials, Region, RequestChecksumCalculation, ResponseChecksumValidation,
};
use aws_sdk_s3::presigning::PresigningConfig;
use chrono::Utc;

use super::{Presigned, Storage, StorageError};
use crate::config::{Config, StorageConfig};

pub struct S3Storage {
    client: aws_sdk_s3::Client,
    bucket: String,
}

impl S3Storage {
    pub fn new(config: &Config) -> Result<Self, StorageError> {
        let StorageConfig::S3 {
            endpoint,
            region,
            bucket,
            access_key_id,
            secret_access_key,
        } = &config.storage
        else {
            return Err(StorageError::Backend(
                "S3Storage requires STORAGE_DRIVER=s3".to_string(),
            ));
        };

        let credentials = Credentials::new(
            access_key_id,
            secret_access_key,
            None,
            None,
            "oci-customer-secret-key",
        );
        let s3_config = aws_sdk_s3::config::Builder::new()
            .behavior_version(BehaviorVersion::latest())
            .region(Region::new(region.clone()))
            .endpoint_url(endpoint)
            .credentials_provider(credentials)
            .force_path_style(true)
            .request_checksum_calculation(RequestChecksumCalculation::WhenRequired)
            .response_checksum_validation(ResponseChecksumValidation::WhenRequired)
            .build();

        Ok(Self {
            client: aws_sdk_s3::Client::from_conf(s3_config),
            bucket: bucket.clone(),
        })
    }

    fn presigning(ttl: Duration) -> Result<PresigningConfig, StorageError> {
        PresigningConfig::expires_in(ttl)
            .map_err(|e| StorageError::Backend(format!("invalid presign TTL: {e}")))
    }
}

#[async_trait]
impl Storage for S3Storage {
    async fn presign_upload(
        &self,
        key: &str,
        content_type: &str,
        ttl: Duration,
    ) -> Result<Presigned, StorageError> {
        let presigned = self
            .client
            .put_object()
            .bucket(&self.bucket)
            .key(key)
            .content_type(content_type)
            .presigned(Self::presigning(ttl)?)
            .await
            .map_err(|e| StorageError::Backend(format!("presign upload failed: {e}")))?;
        Ok(to_presigned(&presigned, ttl))
    }

    async fn presign_download(&self, key: &str, ttl: Duration) -> Result<Presigned, StorageError> {
        let presigned = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(key)
            .presigned(Self::presigning(ttl)?)
            .await
            .map_err(|e| StorageError::Backend(format!("presign download failed: {e}")))?;
        Ok(to_presigned(&presigned, ttl))
    }

    fn name(&self) -> &'static str {
        "s3"
    }
}

fn to_presigned(request: &aws_sdk_s3::presigning::PresignedRequest, ttl: Duration) -> Presigned {
    Presigned {
        url: request.uri().to_string(),
        method: request.method().to_string(),
        headers: request
            .headers()
            .map(|(name, value)| (name.to_string(), value.to_string()))
            .collect(),
        expires_at: Utc::now()
            + chrono::Duration::from_std(ttl).unwrap_or(chrono::Duration::zero()),
    }
}
