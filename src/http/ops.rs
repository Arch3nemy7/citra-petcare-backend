//! Operational endpoints: liveness, readiness, and problem-json fallbacks.
//! These sit outside /api/v1 and outside the OpenAPI document.

use std::time::Duration;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

use crate::http::problem::ProblemDetails;
use crate::state::AppState;

/// Liveness: the process is up and serving.
pub async fn healthz() -> &'static str {
    "ok"
}

/// Readiness: the database answers within a tight deadline. Compose/K8s and
/// nginx health checks should use this one.
pub async fn readyz(State(state): State<AppState>) -> Response {
    let probe = sqlx::query("SELECT 1").execute(&state.db);
    match tokio::time::timeout(Duration::from_secs(2), probe).await {
        Ok(Ok(_)) => (StatusCode::OK, "ready").into_response(),
        Ok(Err(error)) => {
            tracing::warn!(%error, "readiness probe failed");
            ProblemDetails::new(StatusCode::SERVICE_UNAVAILABLE, "database unreachable")
                .with_kind("not-ready")
                .into_response()
        }
        Err(_elapsed) => {
            ProblemDetails::new(StatusCode::SERVICE_UNAVAILABLE, "database probe timed out")
                .with_kind("not-ready")
                .into_response()
        }
    }
}

pub async fn not_found() -> Response {
    ProblemDetails::new(StatusCode::NOT_FOUND, "route not found")
        .with_kind("route-not-found")
        .into_response()
}

pub async fn method_not_allowed() -> Response {
    ProblemDetails::new(
        StatusCode::METHOD_NOT_ALLOWED,
        "method not allowed on this route",
    )
    .with_kind("method-not-allowed")
    .into_response()
}
