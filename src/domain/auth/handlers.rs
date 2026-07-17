use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use super::dto::{LoginRequest, LogoutRequest, RefreshRequest, TokenResponse};
use super::service;
use crate::error::AppError;
use crate::http::extract::ValidatedJson;
use crate::http::problem::ProblemDetails;
use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(login))
        .routes(routes!(refresh))
        .routes(routes!(logout))
}

/// Log in with email + password, receiving an access/refresh token pair.
#[utoipa::path(
    post,
    path = "/api/v1/auth/login",
    tag = "auth",
    request_body = LoginRequest,
    responses(
        (status = 200, description = "Authenticated", body = TokenResponse),
        (status = 401, description = "Invalid credentials", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 422, description = "Validation failed", body = ProblemDetails, content_type = "application/problem+json"),
    )
)]
pub async fn login(
    State(state): State<AppState>,
    ValidatedJson(body): ValidatedJson<LoginRequest>,
) -> Result<Json<TokenResponse>, AppError> {
    let pair = service::login(&state.db, &state.config, &body.email, &body.password).await?;
    Ok(Json(pair.into()))
}

/// Exchange a refresh token for a fresh pair (the old token is revoked).
#[utoipa::path(
    post,
    path = "/api/v1/auth/refresh",
    tag = "auth",
    request_body = RefreshRequest,
    responses(
        (status = 200, description = "Rotated", body = TokenResponse),
        (status = 401, description = "Refresh token invalid, expired, or reused", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 422, description = "Validation failed", body = ProblemDetails, content_type = "application/problem+json"),
    )
)]
pub async fn refresh(
    State(state): State<AppState>,
    ValidatedJson(body): ValidatedJson<RefreshRequest>,
) -> Result<Json<TokenResponse>, AppError> {
    let pair = service::refresh(&state.db, &state.config, &body.refresh_token).await?;
    Ok(Json(pair.into()))
}

/// Revoke a refresh token (the access token simply expires). Possession of
/// the refresh token is the proof of ownership — no access token required,
/// so a device idle past the access-token TTL can still end its session.
#[utoipa::path(
    post,
    path = "/api/v1/auth/logout",
    tag = "auth",
    request_body = LogoutRequest,
    responses(
        (status = 204, description = "Token revoked (idempotent: unknown or malformed tokens also yield 204)"),
    )
)]
pub async fn logout(
    State(state): State<AppState>,
    ValidatedJson(body): ValidatedJson<LogoutRequest>,
) -> Result<StatusCode, AppError> {
    service::logout(&state.db, &body.refresh_token).await?;
    Ok(StatusCode::NO_CONTENT)
}
