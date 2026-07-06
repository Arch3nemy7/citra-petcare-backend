use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use super::dto::{
    InventoryItemRequest, InventoryItemResponse, ListItemsParams, MovementRequest,
    MovementResponse, RecordedMovementResponse,
};
use super::service;
use crate::domain::auth::AuthUser;
use crate::error::AppError;
use crate::http::extract::{ApiPath, ApiQuery, ValidatedJson};
use crate::http::pagination::{PageParams, Paginated, clamp_limit};
use crate::http::problem::ProblemDetails;
use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_items, create_item))
        .routes(routes!(get_item, upsert_item, delete_item))
        .routes(routes!(list_movements, record_movement))
}

/// List inventory items with their derived stock levels.
#[utoipa::path(
    get,
    path = "/api/v1/inventory/items",
    tag = "inventory",
    params(ListItemsParams),
    responses(
        (status = 200, description = "Page of items", body = Paginated<InventoryItemResponse>),
        (status = 401, description = "Not authenticated", body = ProblemDetails, content_type = "application/problem+json"),
    ),
    security(("bearerAuth" = []))
)]
pub async fn list_items(
    State(state): State<AppState>,
    _user: AuthUser,
    ApiQuery(params): ApiQuery<ListItemsParams>,
) -> Result<Json<Paginated<InventoryItemResponse>>, AppError> {
    let limit = clamp_limit(params.limit);
    let page = service::list(
        &state.db,
        params.search.as_deref(),
        params.category,
        params.cursor,
        limit,
    )
    .await?;
    Ok(Json(page.map(InventoryItemResponse::from)))
}

/// Create an inventory item (initial stock arrives via an IN movement).
#[utoipa::path(
    post,
    path = "/api/v1/inventory/items",
    tag = "inventory",
    request_body = InventoryItemRequest,
    responses(
        (status = 201, description = "Created", body = InventoryItemResponse),
        (status = 401, description = "Not authenticated", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 409, description = "Id already exists", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 422, description = "Validation failed", body = ProblemDetails, content_type = "application/problem+json"),
    ),
    security(("bearerAuth" = []))
)]
pub async fn create_item(
    State(state): State<AppState>,
    _user: AuthUser,
    ValidatedJson(body): ValidatedJson<InventoryItemRequest>,
) -> Result<(StatusCode, Json<InventoryItemResponse>), AppError> {
    let item = service::create(&state.db, &body).await?;
    Ok((StatusCode::CREATED, Json(item.into())))
}

/// Fetch one inventory item with its current stock.
#[utoipa::path(
    get,
    path = "/api/v1/inventory/items/{id}",
    tag = "inventory",
    params(("id" = Uuid, Path, description = "Item id")),
    responses(
        (status = 200, description = "Item", body = InventoryItemResponse),
        (status = 401, description = "Not authenticated", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 404, description = "Unknown item", body = ProblemDetails, content_type = "application/problem+json"),
    ),
    security(("bearerAuth" = []))
)]
pub async fn get_item(
    State(state): State<AppState>,
    _user: AuthUser,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<Json<InventoryItemResponse>, AppError> {
    let item = service::get(&state.db, id).await?;
    Ok(Json(item.into()))
}

/// Idempotent upsert (create-or-replace) with a client-generated id.
#[utoipa::path(
    put,
    path = "/api/v1/inventory/items/{id}",
    tag = "inventory",
    params(("id" = Uuid, Path, description = "Client-generated item id")),
    request_body = InventoryItemRequest,
    responses(
        (status = 200, description = "Upserted", body = InventoryItemResponse),
        (status = 401, description = "Not authenticated", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 422, description = "Validation failed", body = ProblemDetails, content_type = "application/problem+json"),
    ),
    security(("bearerAuth" = []))
)]
pub async fn upsert_item(
    State(state): State<AppState>,
    _user: AuthUser,
    ApiPath(id): ApiPath<Uuid>,
    ValidatedJson(body): ValidatedJson<InventoryItemRequest>,
) -> Result<Json<InventoryItemResponse>, AppError> {
    let item = service::upsert(&state.db, id, &body).await?;
    Ok(Json(item.into()))
}

/// Soft-delete an inventory item.
#[utoipa::path(
    delete,
    path = "/api/v1/inventory/items/{id}",
    tag = "inventory",
    params(("id" = Uuid, Path, description = "Item id")),
    responses(
        (status = 204, description = "Deleted"),
        (status = 401, description = "Not authenticated", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 404, description = "Unknown item", body = ProblemDetails, content_type = "application/problem+json"),
    ),
    security(("bearerAuth" = []))
)]
pub async fn delete_item(
    State(state): State<AppState>,
    _user: AuthUser,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<StatusCode, AppError> {
    service::delete(&state.db, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Stock ledger for one item (newest first).
#[utoipa::path(
    get,
    path = "/api/v1/inventory/items/{id}/movements",
    tag = "inventory",
    params(("id" = Uuid, Path, description = "Item id"), PageParams),
    responses(
        (status = 200, description = "Page of movements", body = Paginated<MovementResponse>),
        (status = 401, description = "Not authenticated", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 404, description = "Unknown item", body = ProblemDetails, content_type = "application/problem+json"),
    ),
    security(("bearerAuth" = []))
)]
pub async fn list_movements(
    State(state): State<AppState>,
    _user: AuthUser,
    ApiPath(id): ApiPath<Uuid>,
    ApiQuery(params): ApiQuery<PageParams>,
) -> Result<Json<Paginated<MovementResponse>>, AppError> {
    let limit = clamp_limit(params.limit);
    let page = service::list_movements(&state.db, id, params.cursor, limit).await?;
    Ok(Json(page.map(MovementResponse::from)))
}

/// Record a stock movement (IN, OUT, or signed ADJUSTMENT). OUT movements
/// that would drive stock negative are rejected with 422.
#[utoipa::path(
    post,
    path = "/api/v1/inventory/items/{id}/movements",
    tag = "inventory",
    params(("id" = Uuid, Path, description = "Item id")),
    request_body = MovementRequest,
    responses(
        (status = 201, description = "Recorded", body = RecordedMovementResponse),
        (status = 401, description = "Not authenticated", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 404, description = "Unknown item", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 422, description = "Invalid quantity or insufficient stock", body = ProblemDetails, content_type = "application/problem+json"),
    ),
    security(("bearerAuth" = []))
)]
pub async fn record_movement(
    State(state): State<AppState>,
    _user: AuthUser,
    ApiPath(id): ApiPath<Uuid>,
    ValidatedJson(body): ValidatedJson<MovementRequest>,
) -> Result<(StatusCode, Json<RecordedMovementResponse>), AppError> {
    let (movement, current_stock) = service::record_movement(&state.db, id, &body).await?;
    Ok((
        StatusCode::CREATED,
        Json(RecordedMovementResponse {
            movement: movement.into(),
            current_stock,
        }),
    ))
}
