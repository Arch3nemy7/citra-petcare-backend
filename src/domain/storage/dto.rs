use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::{Validate, ValidationError};

use super::Presigned;

/// File types the clinic app is allowed to upload.
pub const ALLOWED_CONTENT_TYPES: &[&str] = &[
    "image/jpeg",
    "image/png",
    "image/webp",
    "image/heic",
    "image/gif",
    "application/pdf",
];

fn validate_content_type(content_type: &str) -> Result<(), ValidationError> {
    if ALLOWED_CONTENT_TYPES.contains(&content_type) {
        Ok(())
    } else {
        Err(ValidationError::new("content_type")
            .with_message("contentType must be one of: image/jpeg, image/png, image/webp, image/heic, image/gif, application/pdf".into()))
    }
}

#[derive(Debug, Deserialize, Validate, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PresignUploadRequest {
    /// Original file name; used (sanitized) as the tail of the storage key.
    #[validate(length(min = 1, max = 200))]
    #[schema(example = "rontgen-bruno.jpg")]
    pub file_name: String,
    #[validate(custom(function = "validate_content_type"))]
    #[schema(example = "image/jpeg")]
    pub content_type: String,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PresignResponse {
    /// Storage key to persist (e.g. as a patient photoKey or attachment
    /// fileKey). Not returned for downloads of an existing key.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    /// URL to call directly with `method` and `headers`.
    pub url: String,
    pub method: String,
    /// Headers that must be sent verbatim with the request.
    pub headers: BTreeMap<String, String>,
    pub expires_at: DateTime<Utc>,
}

impl PresignResponse {
    pub fn from_presigned(key: Option<String>, presigned: Presigned) -> Self {
        Self {
            key,
            url: presigned.url,
            method: presigned.method,
            headers: presigned.headers,
            expires_at: presigned.expires_at,
        }
    }
}
