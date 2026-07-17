use axum::extract::{FromRequestParts, Request, State};
use axum::http::request::Parts;
use axum::http::{HeaderMap, header};
use axum::middleware::Next;
use axum::response::Response;
use uuid::Uuid;

use super::{AuthError, jwt};
use crate::domain::users::models::UserRole;
use crate::error::AppError;
use crate::state::AppState;

/// Authenticated caller, decoded from the `Authorization: Bearer <jwt>`
/// header by [`require_auth`]. Handlers take this parameter to *use* the
/// identity; the authentication itself already happened at the router level,
/// so a handler cannot accidentally opt out by omitting the parameter.
#[derive(Debug, Clone)]
pub struct AuthUser {
    pub id: Uuid,
    pub email: String,
    pub role: UserRole,
}

/// Router-level auth guard, mounted on the whole protected API via
/// `middleware::from_fn_with_state`: requests without a valid bearer token
/// are rejected with a 401 problem response, valid callers are stored as an
/// [`AuthUser`] request extension.
pub async fn require_auth(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Result<Response, AppError> {
    let user = authenticate(req.headers(), &state)?;
    req.extensions_mut().insert(user);
    Ok(next.run(req).await)
}

fn authenticate(headers: &HeaderMap, state: &AppState) -> Result<AuthUser, AppError> {
    let header_value = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .ok_or(AuthError::MissingToken)?;
    // RFC 7235: the auth scheme is case-insensitive ("bearer" is valid).
    let (scheme, token) = header_value
        .split_once(' ')
        .ok_or(AuthError::MissingToken)?;
    if !scheme.eq_ignore_ascii_case("Bearer") {
        return Err(AuthError::MissingToken.into());
    }
    let claims = jwt::decode_access_token(token.trim_start_matches(' '), &state.config)?;
    Ok(AuthUser {
        id: claims.sub,
        email: claims.email,
        role: claims.role,
    })
}

impl<S: Send + Sync> FromRequestParts<S> for AuthUser {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        // Fail loudly: a handler asking for AuthUser on a route that is not
        // behind `require_auth` is a router wiring bug, not a client error.
        parts.extensions.get::<AuthUser>().cloned().ok_or_else(|| {
            AppError::Internal(
                "handler expects AuthUser but its route is not behind the auth middleware"
                    .to_string(),
            )
        })
    }
}
