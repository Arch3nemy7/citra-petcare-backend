use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use super::dto::{ListNotificationsParams, NotificationResponse};
use super::service;
use crate::domain::auth::AuthUser;
use crate::error::AppError;
use crate::http::extract::{ApiPath, ApiQuery};
use crate::http::pagination::{Paginated, clamp_limit};
use crate::http::problem::ProblemDetails;
use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_notifications))
        .routes(routes!(mark_read))
}

/// List notifications (newest first).
#[utoipa::path(
    get,
    path = "/api/v1/notifications",
    tag = "notifications",
    params(ListNotificationsParams),
    responses(
        (status = 200, description = "Page of notifications", body = Paginated<NotificationResponse>),
        (status = 401, description = "Not authenticated", body = ProblemDetails, content_type = "application/problem+json"),
    ),
    security(("bearerAuth" = []))
)]
pub async fn list_notifications(
    State(state): State<AppState>,
    _user: AuthUser,
    ApiQuery(params): ApiQuery<ListNotificationsParams>,
) -> Result<Json<Paginated<NotificationResponse>>, AppError> {
    let limit = clamp_limit(params.limit);
    let page = service::list(
        &state.db,
        params.unread.unwrap_or(false),
        params.cursor,
        limit,
    )
    .await?;
    Ok(Json(page.map(NotificationResponse::from)))
}

/// Mark a notification as read.
#[utoipa::path(
    post,
    path = "/api/v1/notifications/{id}/read",
    tag = "notifications",
    params(("id" = Uuid, Path, description = "Notification id")),
    responses(
        (status = 204, description = "Marked read"),
        (status = 401, description = "Not authenticated", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 404, description = "Unknown or already-read notification", body = ProblemDetails, content_type = "application/problem+json"),
    ),
    security(("bearerAuth" = []))
)]
pub async fn mark_read(
    State(state): State<AppState>,
    _user: AuthUser,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<StatusCode, AppError> {
    service::mark_read(&state.db, id).await?;
    Ok(StatusCode::NO_CONTENT)
}
