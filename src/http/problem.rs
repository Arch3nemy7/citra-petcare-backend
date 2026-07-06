//! RFC 7807 "problem details" — the single error body shape for every
//! non-2xx response this API produces.

use axum::Json;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use utoipa::ToSchema;

pub const CONTENT_TYPE_PROBLEM_JSON: &str = "application/problem+json";

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProblemDetails {
    /// Stable URN identifying the class of problem — switch on this in the
    /// Flutter client instead of parsing `detail`.
    #[serde(rename = "type")]
    #[schema(example = "urn:citra-petcare:problem:validation")]
    pub problem_type: String,
    /// Human-readable summary matching the HTTP status.
    #[schema(example = "Unprocessable Entity")]
    pub title: String,
    #[schema(example = 422)]
    pub status: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// Per-field validation errors; present only on validation failures.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub errors: Option<serde_json::Value>,
}

impl ProblemDetails {
    pub fn new(status: StatusCode, detail: impl Into<String>) -> Self {
        Self {
            problem_type: "about:blank".to_string(),
            title: status.canonical_reason().unwrap_or("Error").to_string(),
            status: status.as_u16(),
            detail: Some(detail.into()),
            errors: None,
        }
    }

    /// Set the machine-readable problem class, e.g. `with_kind("not-found")`.
    pub fn with_kind(mut self, kind: &str) -> Self {
        self.problem_type = format!("urn:citra-petcare:problem:{kind}");
        self
    }
}

impl IntoResponse for ProblemDetails {
    fn into_response(self) -> Response {
        let status = StatusCode::from_u16(self.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        let mut response = (status, Json(self)).into_response();
        response.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static(CONTENT_TYPE_PROBLEM_JSON),
        );
        response
    }
}
