use axum::Json;
use axum::extract::State;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use super::dto::{ChangesResponse, SyncParams};
use super::service;
use crate::domain::auth::AuthUser;
use crate::error::AppError;
use crate::http::extract::ApiQuery;
use crate::http::problem::ProblemDetails;
use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(changes))
}

/// Offline-sync pull: per-entity rows changed after `since`, including
/// tombstones for soft-deleted rows. Push changes with the entity PUT
/// endpoints (idempotent upserts with client-generated UUIDv7 ids), then pull
/// with the previous response's `serverTime`.
#[utoipa::path(
    get,
    path = "/api/v1/sync/changes",
    tag = "sync",
    params(SyncParams),
    responses(
        (status = 200, description = "Changes since the given instant", body = ChangesResponse),
        (status = 400, description = "Malformed `since`", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 401, description = "Not authenticated", body = ProblemDetails, content_type = "application/problem+json"),
    ),
    security(("bearerAuth" = []))
)]
pub async fn changes(
    State(state): State<AppState>,
    _user: AuthUser,
    ApiQuery(params): ApiQuery<SyncParams>,
) -> Result<Json<ChangesResponse>, AppError> {
    let response = service::changes(&state.db, params.since).await?;
    Ok(Json(response))
}
