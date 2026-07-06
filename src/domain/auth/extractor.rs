use axum::extract::FromRequestParts;
use axum::http::header;
use axum::http::request::Parts;
use uuid::Uuid;

use super::{AuthError, jwt};
use crate::domain::users::models::UserRole;
use crate::error::AppError;
use crate::state::AppState;

/// Authenticated caller, decoded from the `Authorization: Bearer <jwt>`
/// header. Adding this parameter to a handler *is* the auth guard — requests
/// without a valid token are rejected with a 401 problem response before the
/// handler body runs.
#[derive(Debug, Clone)]
pub struct AuthUser {
    pub id: Uuid,
    pub email: String,
    pub role: UserRole,
}

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let header_value = parts
            .headers
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .ok_or(AuthError::MissingToken)?;
        let token = header_value
            .strip_prefix("Bearer ")
            .ok_or(AuthError::MissingToken)?;
        let claims = jwt::decode_access_token(token, &state.config)?;
        Ok(AuthUser {
            id: claims.sub,
            email: claims.email,
            role: claims.role,
        })
    }
}
