use axum::Json;
use axum::extract::State;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use super::dto::DashboardSummaryResponse;
use super::service;
use crate::domain::auth::AuthUser;
use crate::error::AppError;
use crate::http::problem::ProblemDetails;
use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(summary))
}

/// Home-screen summary: today's appointments (Asia/Jakarta), vaccinations due
/// within 14 days or overdue, low-stock items, and items expiring within 60
/// days.
#[utoipa::path(
    get,
    path = "/api/v1/dashboard/summary",
    tag = "dashboard",
    responses(
        (status = 200, description = "Summary", body = DashboardSummaryResponse),
        (status = 401, description = "Not authenticated", body = ProblemDetails, content_type = "application/problem+json"),
    ),
    security(("bearerAuth" = []))
)]
pub async fn summary(
    State(state): State<AppState>,
    _user: AuthUser,
) -> Result<Json<DashboardSummaryResponse>, AppError> {
    let summary = service::summary(&state.db).await?;
    Ok(Json(summary))
}
