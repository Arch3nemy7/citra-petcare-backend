use axum::Json;
use axum::extract::State;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use super::dto::UserResponse;
use super::service;
use crate::domain::auth::AuthUser;
use crate::error::AppError;
use crate::http::problem::ProblemDetails;
use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_users))
        .routes(routes!(me))
}

/// List clinic staff (e.g. to pick the attending vet when recording a visit).
#[utoipa::path(
    get,
    path = "/api/v1/users",
    tag = "users",
    responses(
        (status = 200, description = "All active staff accounts", body = [UserResponse]),
        (status = 401, description = "Not authenticated", body = ProblemDetails, content_type = "application/problem+json"),
    ),
    security(("bearerAuth" = []))
)]
pub async fn list_users(
    State(state): State<AppState>,
    _user: AuthUser,
) -> Result<Json<Vec<UserResponse>>, AppError> {
    let users = service::list_users(&state.db).await?;
    Ok(Json(users.into_iter().map(UserResponse::from).collect()))
}

/// Profile of the authenticated user.
#[utoipa::path(
    get,
    path = "/api/v1/users/me",
    tag = "users",
    responses(
        (status = 200, description = "Current user", body = UserResponse),
        (status = 401, description = "Not authenticated", body = ProblemDetails, content_type = "application/problem+json"),
    ),
    security(("bearerAuth" = []))
)]
pub async fn me(
    State(state): State<AppState>,
    user: AuthUser,
) -> Result<Json<UserResponse>, AppError> {
    let user = service::get_user(&state.db, user.id).await?;
    Ok(Json(user.into()))
}
