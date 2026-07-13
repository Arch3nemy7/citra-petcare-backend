use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use super::dto::{
    BatchUpdateRequest, InventoryItemDetailResponse, InventoryItemRequest, InventoryItemResponse,
    ListItemsParams, MovementRequest, MovementResponse, MovementUpdateRequest,
    RecordedMovementResponse,
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
        .routes(routes!(update_batch))
        .routes(routes!(list_movements, record_movement))
        .routes(routes!(update_movement))
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
        (status = 200, description = "Item with its remaining batches", body = InventoryItemDetailResponse),
        (status = 401, description = "Not authenticated", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 404, description = "Unknown item", body = ProblemDetails, content_type = "application/problem+json"),
    ),
    security(("bearerAuth" = []))
)]
pub async fn get_item(
    State(state): State<AppState>,
    _user: AuthUser,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<Json<InventoryItemDetailResponse>, AppError> {
    let (item, batches) = service::get_detail(&state.db, id).await?;
    Ok(Json(InventoryItemDetailResponse {
        item: item.into(),
        batches: batches.into_iter().map(Into::into).collect(),
    }))
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

/// Correct one batch's expiry date. The batch is identified by its current
/// expiry date + lot number (as shown in the item's batch list); every
/// stock-in that opened it is re-dated and the item's expiry badge is
/// refreshed. Returns the item with its re-derived batches.
#[utoipa::path(
    patch,
    path = "/api/v1/inventory/items/{id}/batches",
    tag = "inventory",
    params(("id" = Uuid, Path, description = "Item id")),
    request_body = BatchUpdateRequest,
    responses(
        (status = 200, description = "Batch re-dated; item with refreshed batches", body = InventoryItemDetailResponse),
        (status = 401, description = "Not authenticated", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 404, description = "Unknown item or batch", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 422, description = "Validation failed", body = ProblemDetails, content_type = "application/problem+json"),
    ),
    security(("bearerAuth" = []))
)]
pub async fn update_batch(
    State(state): State<AppState>,
    _user: AuthUser,
    ApiPath(id): ApiPath<Uuid>,
    ValidatedJson(body): ValidatedJson<BatchUpdateRequest>,
) -> Result<Json<InventoryItemDetailResponse>, AppError> {
    let (item, batches) = service::redate_batch(&state.db, id, &body).await?;
    Ok(Json(InventoryItemDetailResponse {
        item: item.into(),
        batches: batches.into_iter().map(Into::into).collect(),
    }))
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

/// Correct one movement's quantity (a data-entry fix, e.g. 90 → 9).
/// Sign rules for the movement's type still apply; edits that would drive
/// stock negative are rejected, and movements recorded by a visit must be
/// corrected from the visit instead.
#[utoipa::path(
    patch,
    path = "/api/v1/inventory/items/{id}/movements/{movement_id}",
    tag = "inventory",
    params(
        ("id" = Uuid, Path, description = "Item id"),
        ("movement_id" = Uuid, Path, description = "Movement id"),
    ),
    request_body = MovementUpdateRequest,
    responses(
        (status = 200, description = "Updated; entry plus the corrected stock level", body = RecordedMovementResponse),
        (status = 401, description = "Not authenticated", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 404, description = "Unknown item or movement", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 422, description = "Invalid quantity, insufficient stock, or visit-linked movement", body = ProblemDetails, content_type = "application/problem+json"),
    ),
    security(("bearerAuth" = []))
)]
pub async fn update_movement(
    State(state): State<AppState>,
    _user: AuthUser,
    ApiPath((id, movement_id)): ApiPath<(Uuid, Uuid)>,
    ValidatedJson(body): ValidatedJson<MovementUpdateRequest>,
) -> Result<Json<RecordedMovementResponse>, AppError> {
    let (movement, current_stock) =
        service::update_movement_qty(&state.db, id, movement_id, &body).await?;
    Ok(Json(RecordedMovementResponse {
        movement: movement.into(),
        current_stock,
    }))
}
