//! Central application error.
//!
//! Domain modules define their own `thiserror` enums (AuthError,
//! InventoryError, StorageError, NotifyError) and convert into `AppError`
//! via `#[from]`. The single `IntoResponse` impl below turns every error —
//! extractor rejections included — into an RFC 7807 problem response, so no
//! handler ever hand-builds an error body.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

use crate::domain::auth::AuthError;
use crate::domain::inventory::InventoryError;
use crate::domain::notifications::NotifyError;
use crate::domain::storage::StorageError;
use crate::http::problem::ProblemDetails;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0}")]
    BadRequest(String),
    #[error("{0}")]
    Unauthorized(String),
    #[error("{0}")]
    Forbidden(String),
    #[error("{0} not found")]
    NotFound(&'static str),
    #[error("{0}")]
    Conflict(String),
    #[error("{0}")]
    Unprocessable(String),
    #[error("validation failed")]
    Validation(serde_json::Value),
    #[error(transparent)]
    Auth(#[from] AuthError),
    #[error(transparent)]
    Inventory(#[from] InventoryError),
    #[error(transparent)]
    Storage(#[from] StorageError),
    #[error(transparent)]
    Notify(#[from] NotifyError),
    #[error("database error: {0}")]
    Database(sqlx::Error),
    #[error("{0}")]
    Internal(String),
}

impl AppError {
    /// Convert `validator` errors into the 422 Validation variant, carrying
    /// the per-field error map as JSON.
    pub fn validation(errors: validator::ValidationErrors) -> Self {
        Self::Validation(serde_json::to_value(&errors).unwrap_or(serde_json::Value::Null))
    }

    fn status(&self) -> StatusCode {
        match self {
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            Self::Forbidden(_) => StatusCode::FORBIDDEN,
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::Conflict(_) => StatusCode::CONFLICT,
            Self::Unprocessable(_) | Self::Validation(_) | Self::Inventory(_) => {
                StatusCode::UNPROCESSABLE_ENTITY
            }
            Self::Auth(AuthError::Internal(_)) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Auth(_) => StatusCode::UNAUTHORIZED,
            Self::Storage(err) => match err {
                StorageError::InvalidKey(_) => StatusCode::BAD_REQUEST,
                StorageError::SignatureInvalid => StatusCode::UNAUTHORIZED,
                StorageError::NotFound => StatusCode::NOT_FOUND,
                // 500, not 502: Cloudflare replaces origin 502 bodies with
                // its own error page, hiding the problem detail from clients
                // and making storage failures look like proxy outages.
                StorageError::Backend(_) => StatusCode::INTERNAL_SERVER_ERROR,
            },
            Self::Notify(NotifyError::Backend(_)) => StatusCode::BAD_GATEWAY,
            Self::Notify(NotifyError::Config(_)) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Database(_) | Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    /// Machine-readable problem class for the `type` URN.
    fn kind(&self) -> &'static str {
        match self {
            Self::BadRequest(_) => "bad-request",
            Self::Unauthorized(_) | Self::Auth(_) => "unauthorized",
            Self::Forbidden(_) => "forbidden",
            Self::NotFound(_) => "not-found",
            Self::Conflict(_) => "conflict",
            Self::Unprocessable(_) => "unprocessable",
            Self::Validation(_) => "validation",
            Self::Inventory(InventoryError::InsufficientStock { .. }) => "insufficient-stock",
            Self::Inventory(_) => "invalid-quantity",
            Self::Storage(_) => "storage",
            Self::Notify(_) => "notification",
            Self::Database(_) | Self::Internal(_) => "internal",
        }
    }

    /// What clients are allowed to see. 5xx details never leak internals.
    fn public_detail(&self) -> String {
        if self.status().is_server_error() {
            "an internal error occurred".to_string()
        } else {
            self.to_string()
        }
    }
}

/// Classify database errors: constraint violations become client errors with
/// sane statuses instead of opaque 500s.
impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        use sqlx::error::ErrorKind;
        if let Some(db_err) = err.as_database_error() {
            match db_err.kind() {
                ErrorKind::UniqueViolation => {
                    return Self::Conflict(
                        "a record with the same unique value already exists".to_string(),
                    );
                }
                ErrorKind::ForeignKeyViolation => {
                    return Self::Unprocessable("a referenced record does not exist".to_string());
                }
                ErrorKind::CheckViolation | ErrorKind::NotNullViolation => {
                    return Self::Unprocessable("value violates a data constraint".to_string());
                }
                _ => {}
            }
        }
        Self::Database(err)
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = self.status();
        if status.is_server_error() {
            // full detail (incl. SQL errors) goes to the log, never the client
            tracing::error!(error = %self, debug = ?self, "request failed with server error");
        }
        let mut problem = ProblemDetails::new(status, self.public_detail()).with_kind(self.kind());
        if let Self::Validation(errors) = &self {
            problem.errors = Some(errors.clone());
        }
        problem.into_response()
    }
}

#[cfg(test)]
mod tests {
    use axum::http::header;
    use validator::Validate;

    use super::*;

    fn response_for(err: AppError) -> Response {
        err.into_response()
    }

    #[test]
    fn statuses_match_variants() {
        assert_eq!(
            response_for(AppError::NotFound("owner")).status(),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            response_for(AppError::Auth(AuthError::InvalidCredentials)).status(),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            response_for(AppError::Auth(AuthError::Internal("boom".into()))).status(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
        assert_eq!(
            response_for(AppError::Inventory(InventoryError::InsufficientStock {
                requested: 5.0,
                available: 2.0,
            }))
            .status(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(
            response_for(AppError::Conflict("dup".into())).status(),
            StatusCode::CONFLICT
        );
        assert_eq!(
            response_for(AppError::Internal("x".into())).status(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    #[test]
    fn responses_are_problem_json() {
        let response = response_for(AppError::NotFound("patient"));
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE).unwrap(),
            "application/problem+json"
        );
    }

    #[test]
    fn validation_errors_carry_field_map() {
        #[derive(Validate)]
        struct Probe {
            #[validate(length(min = 5))]
            name: String,
        }
        let err = Probe { name: "ab".into() }.validate().unwrap_err();
        let app_err = AppError::validation(err);
        match &app_err {
            AppError::Validation(value) => {
                assert!(value.get("name").is_some(), "field error missing: {value}");
            }
            other => panic!("expected Validation, got {other:?}"),
        }
        assert_eq!(
            app_err.into_response().status(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }

    #[test]
    fn server_errors_hide_details() {
        let err = AppError::Internal("secret stack trace".into());
        assert_eq!(err.public_detail(), "an internal error occurred");
    }
}
