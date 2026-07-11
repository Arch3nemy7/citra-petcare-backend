//! Router assembly: documented /api/v1 routes, ops endpoints, Swagger UI,
//! and the full tower middleware stack.

use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::http::{HeaderValue, Request, header};
use axum::middleware::from_fn;
use axum::routing::get;
use metrics_exporter_prometheus::PrometheusHandle;
use tower::ServiceBuilder;
use tower_governor::GovernorLayer;
use tower_governor::governor::GovernorConfigBuilder;
use tower_governor::key_extractor::SmartIpKeyExtractor;
use tower_http::catch_panic::CatchPanicLayer;
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
use tower_http::sensitive_headers::SetSensitiveRequestHeadersLayer;
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::trace::TraceLayer;
use utoipa::OpenApi;
use utoipa_axum::router::OpenApiRouter;
use utoipa_swagger_ui::SwaggerUi;

use crate::domain::{
    appointments, auth, dashboard, inventory, notifications, owners, patients, storage, sync,
    users, vaccinations, visits,
};
use crate::http::middleware::{
    REQUEST_ID_HEADER, cors_layer, handle_panic, timeout, track_metrics,
};
use crate::http::{openapi, ops};
use crate::state::AppState;

/// Build the complete application router. `metrics` is None in tests (no
/// global Prometheus recorder there).
pub fn build_router(state: AppState, metrics: Option<PrometheusHandle>) -> Router {
    let config = Arc::clone(&state.config);

    // ---- documented business routes (absolute /api/v1/... paths) ----
    let (api, api_doc) = OpenApiRouter::with_openapi(openapi::ApiDoc::openapi())
        .merge(auth::handlers::router())
        .merge(users::handlers::router())
        .merge(owners::handlers::router())
        .merge(patients::handlers::router())
        .merge(visits::handlers::router())
        .merge(vaccinations::handlers::router())
        .merge(appointments::handlers::router())
        .merge(inventory::handlers::router())
        .merge(dashboard::handlers::router())
        .merge(notifications::handlers::router())
        .merge(storage::handlers::router())
        .merge(sync::handlers::router())
        .split_for_parts();

    // Per-IP rate limiting on business routes only (never health checks).
    // SmartIpKeyExtractor prefers X-Forwarded-For/X-Real-Ip (set by nginx),
    // falling back to the socket address.
    let api = if config.rate_limit_enabled {
        let governor_config = Arc::new(
            GovernorConfigBuilder::default()
                .per_second(config.rate_limit_per_second)
                .burst_size(config.rate_limit_burst)
                .key_extractor(SmartIpKeyExtractor)
                .finish()
                .expect("governor configuration is validated at boot"),
        );
        // Periodically evict idle per-IP buckets so the limiter's memory
        // stays bounded.
        let limiter = governor_config.limiter().clone();
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(Duration::from_secs(60));
            loop {
                ticker.tick().await;
                limiter.retain_recent();
            }
        });
        let governor = GovernorLayer::new(governor_config).error_handler(|err| {
            use crate::http::problem::ProblemDetails;
            use axum::http::StatusCode;
            use axum::response::IntoResponse;
            use tower_governor::GovernorError;
            match err {
                GovernorError::TooManyRequests { wait_time, .. } => ProblemDetails::new(
                    StatusCode::TOO_MANY_REQUESTS,
                    format!("rate limit exceeded; retry in {wait_time}s"),
                )
                .with_kind("rate-limited")
                .into_response(),
                GovernorError::UnableToExtractKey => ProblemDetails::new(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "could not determine client address for rate limiting",
                )
                .with_kind("rate-limiter")
                .into_response(),
                GovernorError::Other { .. } => {
                    ProblemDetails::new(StatusCode::INTERNAL_SERVER_ERROR, "rate limiter failure")
                        .with_kind("rate-limiter")
                        .into_response()
                }
            }
        });
        api.layer(governor)
    } else {
        api
    };

    // ---- ops routes + swagger ----
    // The local storage driver's signed upload/download routes sit outside
    // the rate limiter: they are already authenticated by the HMAC signature
    // in the URL, and they carry the actual image bytes — a screen full of
    // photos would otherwise drain the per-IP quota for real API calls.
    let mut app = Router::new()
        .merge(api)
        .merge(storage::handlers::local_router())
        .merge(SwaggerUi::new("/docs").url("/docs/openapi.json", api_doc))
        .route("/healthz", get(ops::healthz))
        .route("/readyz", get(ops::readyz));

    if let Some(handle) = metrics {
        app = app.route(
            "/metrics",
            get(move || {
                let handle = handle.clone();
                async move { handle.render() }
            }),
        );
    }

    // route_layer runs after routing, so MatchedPath is available for
    // bounded-cardinality metric labels (404s to unknown paths are not
    // routed and thus not recorded — by design).
    let app = app
        .route_layer(from_fn(track_metrics))
        .fallback(ops::not_found)
        .method_not_allowed_fallback(ops::method_not_allowed);

    let request_timeout = Duration::from_secs(config.request_timeout_secs);

    // Outermost first. Ordering notes:
    // - request-id is set before tracing so every span carries it
    // - the Authorization header is marked sensitive before anything logs
    // - catch-panic sits inside tracing so panics are logged inside the span
    let middleware = ServiceBuilder::new()
        .layer(SetSensitiveRequestHeadersLayer::new(std::iter::once(
            header::AUTHORIZATION,
        )))
        .layer(SetRequestIdLayer::new(REQUEST_ID_HEADER, MakeRequestUuid))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(|request: &Request<axum::body::Body>| {
                    let request_id = request
                        .headers()
                        .get(REQUEST_ID_HEADER)
                        .and_then(|value| value.to_str().ok())
                        .unwrap_or("-");
                    tracing::info_span!(
                        "http_request",
                        %request_id,
                        method = %request.method(),
                        path = %request.uri().path(),
                    )
                })
                .on_response(
                    // `_` body type: the body at this layer is already
                    // middleware-wrapped, not the plain axum Body
                    |response: &axum::http::Response<_>,
                     latency: Duration,
                     _span: &tracing::Span| {
                        tracing::info!(
                            status = response.status().as_u16(),
                            latency_ms = latency.as_millis() as u64,
                            "response"
                        );
                    },
                ),
        )
        .layer(PropagateRequestIdLayer::new(REQUEST_ID_HEADER))
        .layer(CatchPanicLayer::custom(handle_panic))
        .layer(from_fn(move |req, next| {
            timeout(req, next, request_timeout)
        }))
        .layer(cors_layer(&config))
        .layer(DefaultBodyLimit::max(config.body_limit_bytes))
        .layer(SetResponseHeaderLayer::if_not_present(
            header::X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            header::X_FRAME_OPTIONS,
            HeaderValue::from_static("DENY"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            header::REFERRER_POLICY,
            HeaderValue::from_static("no-referrer"),
        ));

    app.layer(middleware).with_state(state)
}
