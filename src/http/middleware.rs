//! Hand-rolled middleware pieces (metrics, timeout, panic handler, CORS)
//! assembled into the tower stack in `router.rs`.

use std::any::Any;
use std::time::{Duration, Instant};

use axum::extract::{MatchedPath, Request};
use axum::http::{HeaderName, HeaderValue, Method, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use tower_http::cors::{AllowOrigin, CorsLayer};

use crate::config::Config;
use crate::http::problem::ProblemDetails;

pub const REQUEST_ID_HEADER: HeaderName = HeaderName::from_static("x-request-id");

/// Record the Prometheus counter/histogram for each request, labelled by
/// method, matched route *template* (bounded label cardinality — never the
/// raw path) and status. Applied with `route_layer` so it runs after routing,
/// where `MatchedPath` is available in the request extensions.
pub async fn track_metrics(req: Request, next: Next) -> Response {
    let started = Instant::now();
    let method = req.method().to_string();
    let route = req
        .extensions()
        .get::<MatchedPath>()
        .map(|path| path.as_str().to_string())
        .unwrap_or_else(|| "unmatched".to_string());

    let response = next.run(req).await;

    let labels = [
        ("method", method),
        ("route", route),
        ("status", response.status().as_u16().to_string()),
    ];
    metrics::counter!("http_requests_total", &labels).increment(1);
    metrics::histogram!("http_request_duration_seconds", &labels)
        .record(started.elapsed().as_secs_f64());
    response
}

/// Cut off requests that exceed the configured wall-clock budget, answering
/// with a problem response instead of tower-http's empty 408.
pub async fn timeout(req: Request, next: Next, budget: Duration) -> Response {
    match tokio::time::timeout(budget, next.run(req)).await {
        Ok(response) => response,
        Err(_elapsed) => ProblemDetails::new(
            StatusCode::REQUEST_TIMEOUT,
            format!("request exceeded the {}s time budget", budget.as_secs()),
        )
        .with_kind("timeout")
        .into_response(),
    }
}

/// Turn a handler panic into a 500 problem response (and a log line) instead
/// of a dropped connection.
pub fn handle_panic(err: Box<dyn Any + Send + 'static>) -> Response {
    let detail = if let Some(s) = err.downcast_ref::<String>() {
        s.clone()
    } else if let Some(s) = err.downcast_ref::<&str>() {
        (*s).to_string()
    } else {
        "non-string panic payload".to_string()
    };
    tracing::error!(panic = %detail, "handler panicked");
    ProblemDetails::new(
        StatusCode::INTERNAL_SERVER_ERROR,
        "an internal error occurred",
    )
    .with_kind("panic")
    .into_response()
}

/// CORS from the configured allowlist. No allowlist entries ⇒ no browser
/// origins are accepted (mobile apps are unaffected — CORS is browser-only).
pub fn cors_layer(config: &Config) -> CorsLayer {
    let origins: Vec<HeaderValue> = config
        .cors_allowed_origins
        .iter()
        .filter_map(|origin| origin.parse().ok()) // validated at boot
        .collect();
    CorsLayer::new()
        .allow_origin(AllowOrigin::list(origins))
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([
            header::AUTHORIZATION,
            header::CONTENT_TYPE,
            REQUEST_ID_HEADER,
        ])
        .expose_headers([REQUEST_ID_HEADER])
        .max_age(Duration::from_secs(3600))
}
