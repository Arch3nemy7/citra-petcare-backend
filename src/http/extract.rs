//! Extractor wrappers that turn axum's plain-text rejections into RFC 7807
//! problem responses, and wire the `validator` crate into JSON bodies.

use axum::extract::{FromRequest, FromRequestParts, Json, Path, Query, Request};
use axum::http::request::Parts;
use serde::de::DeserializeOwned;
use validator::Validate;

use crate::error::AppError;

/// JSON body extractor that (1) rejects malformed JSON with a problem
/// response and (2) runs `validator` rules, answering 422 with a per-field
/// error map on failure. Handlers therefore only ever see valid input.
pub struct ValidatedJson<T>(pub T);

impl<S, T> FromRequest<S> for ValidatedJson<T>
where
    S: Send + Sync,
    T: DeserializeOwned + Validate,
{
    type Rejection = AppError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let Json(value) = Json::<T>::from_request(req, state)
            .await
            .map_err(|rejection| {
                AppError::BadRequest(format!("invalid JSON body: {}", rejection.body_text()))
            })?;
        value.validate().map_err(AppError::validation)?;
        Ok(Self(value))
    }
}

/// `Query` with problem-response rejections.
pub struct ApiQuery<T>(pub T);

impl<S, T> FromRequestParts<S> for ApiQuery<T>
where
    S: Send + Sync,
    T: DeserializeOwned + Send,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let Query(value) =
            Query::<T>::from_request_parts(parts, state)
                .await
                .map_err(|rejection| {
                    AppError::BadRequest(format!("invalid query string: {}", rejection.body_text()))
                })?;
        Ok(Self(value))
    }
}

/// `Path` with problem-response rejections (e.g. a malformed UUID → 400
/// problem+json instead of axum's plain-text 400).
pub struct ApiPath<T>(pub T);

impl<S, T> FromRequestParts<S> for ApiPath<T>
where
    S: Send + Sync,
    T: DeserializeOwned + Send,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let Path(value) =
            Path::<T>::from_request_parts(parts, state)
                .await
                .map_err(|rejection| {
                    AppError::BadRequest(format!(
                        "invalid path parameter: {}",
                        rejection.body_text()
                    ))
                })?;
        Ok(Self(value))
    }
}
