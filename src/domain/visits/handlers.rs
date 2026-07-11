use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use super::dto::{
    AttachmentRequest, ListVisitsParams, VisitAttachmentResponse, VisitDetailResponse,
    VisitRequest, VisitResponse, VisitStockUsageResponse,
};
use super::service;
use crate::domain::auth::AuthUser;
use crate::domain::patients;
use crate::error::AppError;
use crate::http::extract::{ApiPath, ApiQuery, ValidatedJson};
use crate::http::pagination::{PageParams, Paginated, clamp_limit};
use crate::http::problem::ProblemDetails;
use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_visits, create_visit))
        .routes(routes!(get_visit, upsert_visit, delete_visit))
        .routes(routes!(add_attachment))
        .routes(routes!(delete_attachment))
        .routes(routes!(list_patient_visits))
}

/// List visits (all patients, or filtered), ordered by visit date descending.
#[utoipa::path(
    get,
    path = "/api/v1/visits",
    tag = "visits",
    params(ListVisitsParams),
    responses(
        (status = 200, description = "Page of visits", body = Paginated<VisitResponse>),
        (status = 401, description = "Not authenticated", body = ProblemDetails, content_type = "application/problem+json"),
    ),
    security(("bearerAuth" = []))
)]
pub async fn list_visits(
    State(state): State<AppState>,
    _user: AuthUser,
    ApiQuery(params): ApiQuery<ListVisitsParams>,
) -> Result<Json<Paginated<VisitResponse>>, AppError> {
    let limit = clamp_limit(params.limit);
    let page = service::list(
        &state.db,
        params.patient_id,
        params.from,
        params.to,
        params.cursor,
        limit,
    )
    .await?;
    Ok(Json(page.map(VisitResponse::from)))
}

/// Record a visit.
#[utoipa::path(
    post,
    path = "/api/v1/visits",
    tag = "visits",
    request_body = VisitRequest,
    responses(
        (status = 201, description = "Created", body = VisitResponse),
        (status = 401, description = "Not authenticated", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 409, description = "Id already exists", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 422, description = "Validation failed / unknown patient or vet", body = ProblemDetails, content_type = "application/problem+json"),
    ),
    security(("bearerAuth" = []))
)]
pub async fn create_visit(
    State(state): State<AppState>,
    _user: AuthUser,
    ValidatedJson(body): ValidatedJson<VisitRequest>,
) -> Result<(StatusCode, Json<VisitResponse>), AppError> {
    let visit = service::create(&state.db, &body).await?;
    Ok((StatusCode::CREATED, Json(visit.into())))
}

/// Fetch one visit, including its attachments and the stock it consumed.
#[utoipa::path(
    get,
    path = "/api/v1/visits/{id}",
    tag = "visits",
    params(("id" = Uuid, Path, description = "Visit id")),
    responses(
        (status = 200, description = "Visit with attachments and stock usage", body = VisitDetailResponse),
        (status = 401, description = "Not authenticated", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 404, description = "Unknown visit", body = ProblemDetails, content_type = "application/problem+json"),
    ),
    security(("bearerAuth" = []))
)]
pub async fn get_visit(
    State(state): State<AppState>,
    _user: AuthUser,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<Json<VisitDetailResponse>, AppError> {
    let (visit, attachments, stock_usage) = service::get_detail(&state.db, id).await?;
    Ok(Json(VisitDetailResponse {
        visit: visit.into(),
        attachments: attachments
            .into_iter()
            .map(VisitAttachmentResponse::from)
            .collect(),
        stock_usage: stock_usage
            .into_iter()
            .map(VisitStockUsageResponse::from)
            .collect(),
    }))
}

/// Idempotent upsert (create-or-replace) with a client-generated id.
#[utoipa::path(
    put,
    path = "/api/v1/visits/{id}",
    tag = "visits",
    params(("id" = Uuid, Path, description = "Client-generated visit id")),
    request_body = VisitRequest,
    responses(
        (status = 200, description = "Upserted", body = VisitResponse),
        (status = 401, description = "Not authenticated", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 422, description = "Validation failed / unknown patient or vet", body = ProblemDetails, content_type = "application/problem+json"),
    ),
    security(("bearerAuth" = []))
)]
pub async fn upsert_visit(
    State(state): State<AppState>,
    _user: AuthUser,
    ApiPath(id): ApiPath<Uuid>,
    ValidatedJson(body): ValidatedJson<VisitRequest>,
) -> Result<Json<VisitResponse>, AppError> {
    let visit = service::upsert(&state.db, id, &body).await?;
    Ok(Json(visit.into()))
}

/// Soft-delete a visit (its attachments are tombstoned too).
#[utoipa::path(
    delete,
    path = "/api/v1/visits/{id}",
    tag = "visits",
    params(("id" = Uuid, Path, description = "Visit id")),
    responses(
        (status = 204, description = "Deleted"),
        (status = 401, description = "Not authenticated", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 404, description = "Unknown visit", body = ProblemDetails, content_type = "application/problem+json"),
    ),
    security(("bearerAuth" = []))
)]
pub async fn delete_visit(
    State(state): State<AppState>,
    _user: AuthUser,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<StatusCode, AppError> {
    service::delete(&state.db, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Attach an uploaded file (photo/x-ray/lab result) to a visit.
#[utoipa::path(
    post,
    path = "/api/v1/visits/{id}/attachments",
    tag = "visits",
    params(("id" = Uuid, Path, description = "Visit id")),
    request_body = AttachmentRequest,
    responses(
        (status = 201, description = "Attached", body = VisitAttachmentResponse),
        (status = 401, description = "Not authenticated", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 404, description = "Unknown visit", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 422, description = "Validation failed", body = ProblemDetails, content_type = "application/problem+json"),
    ),
    security(("bearerAuth" = []))
)]
pub async fn add_attachment(
    State(state): State<AppState>,
    _user: AuthUser,
    ApiPath(id): ApiPath<Uuid>,
    ValidatedJson(body): ValidatedJson<AttachmentRequest>,
) -> Result<(StatusCode, Json<VisitAttachmentResponse>), AppError> {
    let attachment = service::add_attachment(&state.db, id, &body).await?;
    Ok((StatusCode::CREATED, Json(attachment.into())))
}

/// Detach (soft-delete) an attachment from a visit.
#[utoipa::path(
    delete,
    path = "/api/v1/visits/{id}/attachments/{attachmentId}",
    tag = "visits",
    params(
        ("id" = Uuid, Path, description = "Visit id"),
        ("attachmentId" = Uuid, Path, description = "Attachment id"),
    ),
    responses(
        (status = 204, description = "Detached"),
        (status = 401, description = "Not authenticated", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 404, description = "Unknown visit or attachment", body = ProblemDetails, content_type = "application/problem+json"),
    ),
    security(("bearerAuth" = []))
)]
pub async fn delete_attachment(
    State(state): State<AppState>,
    _user: AuthUser,
    ApiPath((id, attachment_id)): ApiPath<(Uuid, Uuid)>,
) -> Result<StatusCode, AppError> {
    service::remove_attachment(&state.db, id, attachment_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// List one patient's visits (newest first).
#[utoipa::path(
    get,
    path = "/api/v1/patients/{id}/visits",
    tag = "visits",
    params(("id" = Uuid, Path, description = "Patient id"), PageParams),
    responses(
        (status = 200, description = "Page of visits", body = Paginated<VisitResponse>),
        (status = 401, description = "Not authenticated", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 404, description = "Unknown patient", body = ProblemDetails, content_type = "application/problem+json"),
    ),
    security(("bearerAuth" = []))
)]
pub async fn list_patient_visits(
    State(state): State<AppState>,
    _user: AuthUser,
    ApiPath(id): ApiPath<Uuid>,
    ApiQuery(params): ApiQuery<PageParams>,
) -> Result<Json<Paginated<VisitResponse>>, AppError> {
    patients::service::get(&state.db, id).await?; // 404 for unknown patients
    let limit = clamp_limit(params.limit);
    let page = service::list(&state.db, Some(id), None, None, params.cursor, limit).await?;
    Ok(Json(page.map(VisitResponse::from)))
}
