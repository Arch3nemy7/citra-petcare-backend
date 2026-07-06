use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use super::dto::{CreateVaccinationRequest, UpsertVaccinationRequest, VaccinationResponse};
use super::service;
use crate::domain::auth::AuthUser;
use crate::error::AppError;
use crate::http::extract::{ApiPath, ApiQuery, ValidatedJson};
use crate::http::pagination::{PageParams, Paginated, clamp_limit};
use crate::http::problem::ProblemDetails;
use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_patient_vaccinations, create_vaccination))
        .routes(routes!(
            get_vaccination,
            upsert_vaccination,
            delete_vaccination
        ))
}

/// Vaccination history for one patient (most recent first).
#[utoipa::path(
    get,
    path = "/api/v1/patients/{id}/vaccinations",
    tag = "vaccinations",
    params(("id" = Uuid, Path, description = "Patient id"), PageParams),
    responses(
        (status = 200, description = "Page of vaccinations", body = Paginated<VaccinationResponse>),
        (status = 401, description = "Not authenticated", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 404, description = "Unknown patient", body = ProblemDetails, content_type = "application/problem+json"),
    ),
    security(("bearerAuth" = []))
)]
pub async fn list_patient_vaccinations(
    State(state): State<AppState>,
    _user: AuthUser,
    ApiPath(id): ApiPath<Uuid>,
    ApiQuery(params): ApiQuery<PageParams>,
) -> Result<Json<Paginated<VaccinationResponse>>, AppError> {
    let limit = clamp_limit(params.limit);
    let page = service::list_for_patient(&state.db, id, params.cursor, limit).await?;
    Ok(Json(page.map(VaccinationResponse::from)))
}

/// Record a vaccination for a patient.
#[utoipa::path(
    post,
    path = "/api/v1/patients/{id}/vaccinations",
    tag = "vaccinations",
    params(("id" = Uuid, Path, description = "Patient id")),
    request_body = CreateVaccinationRequest,
    responses(
        (status = 201, description = "Recorded", body = VaccinationResponse),
        (status = 401, description = "Not authenticated", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 404, description = "Unknown patient", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 422, description = "Validation failed", body = ProblemDetails, content_type = "application/problem+json"),
    ),
    security(("bearerAuth" = []))
)]
pub async fn create_vaccination(
    State(state): State<AppState>,
    _user: AuthUser,
    ApiPath(id): ApiPath<Uuid>,
    ValidatedJson(body): ValidatedJson<CreateVaccinationRequest>,
) -> Result<(StatusCode, Json<VaccinationResponse>), AppError> {
    let vaccination = service::create_for_patient(&state.db, id, &body).await?;
    Ok((StatusCode::CREATED, Json(vaccination.into())))
}

/// Fetch one vaccination record.
#[utoipa::path(
    get,
    path = "/api/v1/vaccinations/{id}",
    tag = "vaccinations",
    params(("id" = Uuid, Path, description = "Vaccination id")),
    responses(
        (status = 200, description = "Vaccination", body = VaccinationResponse),
        (status = 401, description = "Not authenticated", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 404, description = "Unknown vaccination", body = ProblemDetails, content_type = "application/problem+json"),
    ),
    security(("bearerAuth" = []))
)]
pub async fn get_vaccination(
    State(state): State<AppState>,
    _user: AuthUser,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<Json<VaccinationResponse>, AppError> {
    let vaccination = service::get(&state.db, id).await?;
    Ok(Json(vaccination.into()))
}

/// Idempotent upsert (create-or-replace) with a client-generated id.
#[utoipa::path(
    put,
    path = "/api/v1/vaccinations/{id}",
    tag = "vaccinations",
    params(("id" = Uuid, Path, description = "Client-generated vaccination id")),
    request_body = UpsertVaccinationRequest,
    responses(
        (status = 200, description = "Upserted", body = VaccinationResponse),
        (status = 401, description = "Not authenticated", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 422, description = "Validation failed / unknown patient", body = ProblemDetails, content_type = "application/problem+json"),
    ),
    security(("bearerAuth" = []))
)]
pub async fn upsert_vaccination(
    State(state): State<AppState>,
    _user: AuthUser,
    ApiPath(id): ApiPath<Uuid>,
    ValidatedJson(body): ValidatedJson<UpsertVaccinationRequest>,
) -> Result<Json<VaccinationResponse>, AppError> {
    let vaccination = service::upsert(&state.db, id, &body).await?;
    Ok(Json(vaccination.into()))
}

/// Soft-delete a vaccination record.
#[utoipa::path(
    delete,
    path = "/api/v1/vaccinations/{id}",
    tag = "vaccinations",
    params(("id" = Uuid, Path, description = "Vaccination id")),
    responses(
        (status = 204, description = "Deleted"),
        (status = 401, description = "Not authenticated", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 404, description = "Unknown vaccination", body = ProblemDetails, content_type = "application/problem+json"),
    ),
    security(("bearerAuth" = []))
)]
pub async fn delete_vaccination(
    State(state): State<AppState>,
    _user: AuthUser,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<StatusCode, AppError> {
    service::delete(&state.db, id).await?;
    Ok(StatusCode::NO_CONTENT)
}
