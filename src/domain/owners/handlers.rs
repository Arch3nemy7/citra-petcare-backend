use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use super::dto::{ListOwnersParams, OwnerRequest, OwnerResponse};
use super::service;
use crate::domain::auth::AuthUser;
use crate::error::AppError;
use crate::http::extract::{ApiPath, ApiQuery, ValidatedJson};
use crate::http::pagination::{Paginated, clamp_limit};
use crate::http::problem::ProblemDetails;
use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_owners, create_owner))
        .routes(routes!(get_owner, upsert_owner, delete_owner))
}

/// List owners, newest first, with optional name/phone search.
#[utoipa::path(
    get,
    path = "/api/v1/owners",
    tag = "owners",
    params(ListOwnersParams),
    responses(
        (status = 200, description = "Page of owners", body = Paginated<OwnerResponse>),
        (status = 401, description = "Not authenticated", body = ProblemDetails, content_type = "application/problem+json"),
    ),
    security(("bearerAuth" = []))
)]
pub async fn list_owners(
    State(state): State<AppState>,
    _user: AuthUser,
    ApiQuery(params): ApiQuery<ListOwnersParams>,
) -> Result<Json<Paginated<OwnerResponse>>, AppError> {
    let limit = clamp_limit(params.limit);
    let page = service::list(&state.db, params.search.as_deref(), params.cursor, limit).await?;
    Ok(Json(page.map(OwnerResponse::from)))
}

/// Create an owner. The id may be supplied by the client (offline sync).
#[utoipa::path(
    post,
    path = "/api/v1/owners",
    tag = "owners",
    request_body = OwnerRequest,
    responses(
        (status = 201, description = "Created", body = OwnerResponse),
        (status = 401, description = "Not authenticated", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 409, description = "Id already exists", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 422, description = "Validation failed", body = ProblemDetails, content_type = "application/problem+json"),
    ),
    security(("bearerAuth" = []))
)]
pub async fn create_owner(
    State(state): State<AppState>,
    _user: AuthUser,
    ValidatedJson(body): ValidatedJson<OwnerRequest>,
) -> Result<(StatusCode, Json<OwnerResponse>), AppError> {
    let owner = service::create(&state.db, &body).await?;
    Ok((StatusCode::CREATED, Json(owner.into())))
}

/// Fetch one owner.
#[utoipa::path(
    get,
    path = "/api/v1/owners/{id}",
    tag = "owners",
    params(("id" = Uuid, Path, description = "Owner id")),
    responses(
        (status = 200, description = "Owner", body = OwnerResponse),
        (status = 401, description = "Not authenticated", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 404, description = "Unknown owner", body = ProblemDetails, content_type = "application/problem+json"),
    ),
    security(("bearerAuth" = []))
)]
pub async fn get_owner(
    State(state): State<AppState>,
    _user: AuthUser,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<Json<OwnerResponse>, AppError> {
    let owner = service::get(&state.db, id).await?;
    Ok(Json(owner.into()))
}

/// Idempotent upsert (create-or-replace) — the write primitive used by
/// offline sync. Re-upserting a soft-deleted owner resurrects it.
#[utoipa::path(
    put,
    path = "/api/v1/owners/{id}",
    tag = "owners",
    params(("id" = Uuid, Path, description = "Client-generated owner id")),
    request_body = OwnerRequest,
    responses(
        (status = 200, description = "Upserted", body = OwnerResponse),
        (status = 401, description = "Not authenticated", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 422, description = "Validation failed", body = ProblemDetails, content_type = "application/problem+json"),
    ),
    security(("bearerAuth" = []))
)]
pub async fn upsert_owner(
    State(state): State<AppState>,
    _user: AuthUser,
    ApiPath(id): ApiPath<Uuid>,
    ValidatedJson(body): ValidatedJson<OwnerRequest>,
) -> Result<Json<OwnerResponse>, AppError> {
    let owner = service::upsert(&state.db, id, &body).await?;
    Ok(Json(owner.into()))
}

/// Soft-delete an owner. Fails with 409 while the owner still has patients.
#[utoipa::path(
    delete,
    path = "/api/v1/owners/{id}",
    tag = "owners",
    params(("id" = Uuid, Path, description = "Owner id")),
    responses(
        (status = 204, description = "Deleted"),
        (status = 401, description = "Not authenticated", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 404, description = "Unknown owner", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 409, description = "Owner still has patients", body = ProblemDetails, content_type = "application/problem+json"),
    ),
    security(("bearerAuth" = []))
)]
pub async fn delete_owner(
    State(state): State<AppState>,
    _user: AuthUser,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<StatusCode, AppError> {
    service::delete(&state.db, id).await?;
    Ok(StatusCode::NO_CONTENT)
}
