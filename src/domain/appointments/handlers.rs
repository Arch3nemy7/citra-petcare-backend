use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use super::dto::{AppointmentRequest, AppointmentResponse, ListAppointmentsParams};
use super::service;
use crate::domain::auth::AuthUser;
use crate::error::AppError;
use crate::http::extract::{ApiPath, ApiQuery, ValidatedJson};
use crate::http::pagination::{Paginated, clamp_limit};
use crate::http::problem::ProblemDetails;
use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_appointments, create_appointment))
        .routes(routes!(
            get_appointment,
            upsert_appointment,
            delete_appointment
        ))
}

/// Agenda: list appointments, soonest first, filterable by window and status.
#[utoipa::path(
    get,
    path = "/api/v1/appointments",
    tag = "appointments",
    params(ListAppointmentsParams),
    responses(
        (status = 200, description = "Page of appointments", body = Paginated<AppointmentResponse>),
        (status = 401, description = "Not authenticated", body = ProblemDetails, content_type = "application/problem+json"),
    ),
    security(("bearerAuth" = []))
)]
pub async fn list_appointments(
    State(state): State<AppState>,
    _user: AuthUser,
    ApiQuery(params): ApiQuery<ListAppointmentsParams>,
) -> Result<Json<Paginated<AppointmentResponse>>, AppError> {
    let limit = clamp_limit(params.limit);
    let page = service::list(
        &state.db,
        params.from,
        params.to,
        params.status,
        params.patient_id,
        params.cursor,
        limit,
    )
    .await?;
    Ok(Json(page.map(AppointmentResponse::from)))
}

/// Book an appointment.
#[utoipa::path(
    post,
    path = "/api/v1/appointments",
    tag = "appointments",
    request_body = AppointmentRequest,
    responses(
        (status = 201, description = "Booked", body = AppointmentResponse),
        (status = 401, description = "Not authenticated", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 409, description = "Id already exists", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 422, description = "Validation failed / unknown patient", body = ProblemDetails, content_type = "application/problem+json"),
    ),
    security(("bearerAuth" = []))
)]
pub async fn create_appointment(
    State(state): State<AppState>,
    _user: AuthUser,
    ValidatedJson(body): ValidatedJson<AppointmentRequest>,
) -> Result<(StatusCode, Json<AppointmentResponse>), AppError> {
    let appointment = service::create(&state.db, &body).await?;
    Ok((StatusCode::CREATED, Json(appointment.into())))
}

/// Fetch one appointment.
#[utoipa::path(
    get,
    path = "/api/v1/appointments/{id}",
    tag = "appointments",
    params(("id" = Uuid, Path, description = "Appointment id")),
    responses(
        (status = 200, description = "Appointment", body = AppointmentResponse),
        (status = 401, description = "Not authenticated", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 404, description = "Unknown appointment", body = ProblemDetails, content_type = "application/problem+json"),
    ),
    security(("bearerAuth" = []))
)]
pub async fn get_appointment(
    State(state): State<AppState>,
    _user: AuthUser,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<Json<AppointmentResponse>, AppError> {
    let appointment = service::get(&state.db, id).await?;
    Ok(Json(appointment.into()))
}

/// Idempotent upsert — also how the app reschedules or marks DONE/NO_SHOW.
#[utoipa::path(
    put,
    path = "/api/v1/appointments/{id}",
    tag = "appointments",
    params(("id" = Uuid, Path, description = "Client-generated appointment id")),
    request_body = AppointmentRequest,
    responses(
        (status = 200, description = "Upserted", body = AppointmentResponse),
        (status = 401, description = "Not authenticated", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 422, description = "Validation failed / unknown patient", body = ProblemDetails, content_type = "application/problem+json"),
    ),
    security(("bearerAuth" = []))
)]
pub async fn upsert_appointment(
    State(state): State<AppState>,
    _user: AuthUser,
    ApiPath(id): ApiPath<Uuid>,
    ValidatedJson(body): ValidatedJson<AppointmentRequest>,
) -> Result<Json<AppointmentResponse>, AppError> {
    let appointment = service::upsert(&state.db, id, &body).await?;
    Ok(Json(appointment.into()))
}

/// Soft-delete an appointment (prefer status=CANCELLED to keep history).
#[utoipa::path(
    delete,
    path = "/api/v1/appointments/{id}",
    tag = "appointments",
    params(("id" = Uuid, Path, description = "Appointment id")),
    responses(
        (status = 204, description = "Deleted"),
        (status = 401, description = "Not authenticated", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 404, description = "Unknown appointment", body = ProblemDetails, content_type = "application/problem+json"),
    ),
    security(("bearerAuth" = []))
)]
pub async fn delete_appointment(
    State(state): State<AppState>,
    _user: AuthUser,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<StatusCode, AppError> {
    service::delete(&state.db, id).await?;
    Ok(StatusCode::NO_CONTENT)
}
