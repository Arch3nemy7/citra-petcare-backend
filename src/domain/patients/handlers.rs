use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use super::dto::{ListPatientsParams, PatientRequest, PatientResponse, WeightPointResponse};
use super::service;
use crate::domain::auth::AuthUser;
use crate::error::AppError;
use crate::http::extract::{ApiPath, ApiQuery, ValidatedJson};
use crate::http::pagination::{Paginated, clamp_limit};
use crate::http::problem::ProblemDetails;
use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_patients, create_patient))
        .routes(routes!(get_patient, upsert_patient, delete_patient))
        .routes(routes!(weight_history))
}

/// List patients, newest first. Search matches patient name, owner name and
/// owner phone.
#[utoipa::path(
    get,
    path = "/api/v1/patients",
    tag = "patients",
    params(ListPatientsParams),
    responses(
        (status = 200, description = "Page of patients", body = Paginated<PatientResponse>),
        (status = 401, description = "Not authenticated", body = ProblemDetails, content_type = "application/problem+json"),
    ),
    security(("bearerAuth" = []))
)]
pub async fn list_patients(
    State(state): State<AppState>,
    _user: AuthUser,
    ApiQuery(params): ApiQuery<ListPatientsParams>,
) -> Result<Json<Paginated<PatientResponse>>, AppError> {
    let limit = clamp_limit(params.limit);
    let page = service::list(
        &state.db,
        params.search.as_deref(),
        params.owner_id,
        params.cursor,
        limit,
    )
    .await?;
    Ok(Json(page.map(PatientResponse::from)))
}

/// Register a patient.
#[utoipa::path(
    post,
    path = "/api/v1/patients",
    tag = "patients",
    request_body = PatientRequest,
    responses(
        (status = 201, description = "Created", body = PatientResponse),
        (status = 401, description = "Not authenticated", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 409, description = "Id already exists", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 422, description = "Validation failed / unknown owner", body = ProblemDetails, content_type = "application/problem+json"),
    ),
    security(("bearerAuth" = []))
)]
pub async fn create_patient(
    State(state): State<AppState>,
    _user: AuthUser,
    ValidatedJson(body): ValidatedJson<PatientRequest>,
) -> Result<(StatusCode, Json<PatientResponse>), AppError> {
    let patient = service::create(&state.db, &body).await?;
    Ok((StatusCode::CREATED, Json(patient.into())))
}

/// Fetch one patient.
#[utoipa::path(
    get,
    path = "/api/v1/patients/{id}",
    tag = "patients",
    params(("id" = Uuid, Path, description = "Patient id")),
    responses(
        (status = 200, description = "Patient", body = PatientResponse),
        (status = 401, description = "Not authenticated", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 404, description = "Unknown patient", body = ProblemDetails, content_type = "application/problem+json"),
    ),
    security(("bearerAuth" = []))
)]
pub async fn get_patient(
    State(state): State<AppState>,
    _user: AuthUser,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<Json<PatientResponse>, AppError> {
    let patient = service::get(&state.db, id).await?;
    Ok(Json(patient.into()))
}

/// Idempotent upsert (create-or-replace) with a client-generated id.
#[utoipa::path(
    put,
    path = "/api/v1/patients/{id}",
    tag = "patients",
    params(("id" = Uuid, Path, description = "Client-generated patient id")),
    request_body = PatientRequest,
    responses(
        (status = 200, description = "Upserted", body = PatientResponse),
        (status = 401, description = "Not authenticated", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 422, description = "Validation failed / unknown owner", body = ProblemDetails, content_type = "application/problem+json"),
    ),
    security(("bearerAuth" = []))
)]
pub async fn upsert_patient(
    State(state): State<AppState>,
    _user: AuthUser,
    ApiPath(id): ApiPath<Uuid>,
    ValidatedJson(body): ValidatedJson<PatientRequest>,
) -> Result<Json<PatientResponse>, AppError> {
    let patient = service::upsert(&state.db, id, &body).await?;
    Ok(Json(patient.into()))
}

/// Soft-delete a patient (history stays available through sync tombstones).
#[utoipa::path(
    delete,
    path = "/api/v1/patients/{id}",
    tag = "patients",
    params(("id" = Uuid, Path, description = "Patient id")),
    responses(
        (status = 204, description = "Deleted"),
        (status = 401, description = "Not authenticated", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 404, description = "Unknown patient", body = ProblemDetails, content_type = "application/problem+json"),
    ),
    security(("bearerAuth" = []))
)]
pub async fn delete_patient(
    State(state): State<AppState>,
    _user: AuthUser,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<StatusCode, AppError> {
    service::delete(&state.db, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Weight measurements over time, taken from visit records.
#[utoipa::path(
    get,
    path = "/api/v1/patients/{id}/weight-history",
    tag = "patients",
    params(("id" = Uuid, Path, description = "Patient id")),
    responses(
        (status = 200, description = "Chronological weight series", body = [WeightPointResponse]),
        (status = 401, description = "Not authenticated", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 404, description = "Unknown patient", body = ProblemDetails, content_type = "application/problem+json"),
    ),
    security(("bearerAuth" = []))
)]
pub async fn weight_history(
    State(state): State<AppState>,
    _user: AuthUser,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<Json<Vec<WeightPointResponse>>, AppError> {
    let points = service::weight_history(&state.db, id).await?;
    Ok(Json(
        points
            .into_iter()
            .map(|p| WeightPointResponse {
                visit_id: p.visit_id,
                visit_date: p.visit_date,
                weight_kg: p.weight_kg,
            })
            .collect(),
    ))
}
