//! Router assembly: documented /api/v1 routes, ops endpoints, Swagger UI,
//! and the full tower middleware stack.

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::extract::{ConnectInfo, DefaultBodyLimit};
use axum::http::{HeaderValue, Request, header};
use axum::middleware::{from_fn, from_fn_with_state};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use metrics_exporter_prometheus::PrometheusHandle;
use tower::ServiceBuilder;
use tower_governor::governor::GovernorConfigBuilder;
use tower_governor::key_extractor::{KeyExtractor, SmartIpKeyExtractor};
use tower_governor::{GovernorError, GovernorLayer};
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

/// Rate-limit key: the real client address. Cloudflare fronts production
/// and carries the visitor IP in CF-Connecting-IP (always exactly one
/// value). The X-Forwarded-For chain is rewritten per proxy hop and, as
/// observed live, keys buckets by Cloudflare *edge node* instead — one
/// phone then shares its quota with every request relayed through that
/// edge and trips 429s during ordinary use. Falls back to the standard
/// headers/peer address for non-Cloudflare traffic (local dev, tests).
///
/// SECURITY: CF-Connecting-IP is client-supplied and only trustworthy because
/// the fronting reverse proxy *overwrites* it with the Cloudflare-validated
/// `$remote_addr` before proxying (see `proxy_set_header CF-Connecting-IP`
/// in `deploy/nginx/…conf`), and/or the origin only accepts connections from
/// Cloudflare IP ranges. Without one of those, a caller reaching the origin
/// directly could rotate a forged header per request to mint a fresh bucket
/// and defeat the limiter (including the strict auth bucket). Keep that
/// sanitization/firewalling in place for every deployment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ClientIpKeyExtractor;

impl KeyExtractor for ClientIpKeyExtractor {
    type Key = IpAddr;

    fn extract<T>(&self, req: &Request<T>) -> Result<Self::Key, GovernorError> {
        let cf_ip: Option<IpAddr> = req
            .headers()
            .get("cf-connecting-ip")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.trim().parse().ok());
        match cf_ip {
            Some(ip) => Ok(ip),
            None => SmartIpKeyExtractor.extract(req),
        }
    }
}

/// 429 (and key-extraction failures) as RFC 7807 problem bodies.
fn rate_limit_error(err: GovernorError) -> Response {
    use crate::http::problem::ProblemDetails;
    use axum::http::StatusCode;
    use axum::response::IntoResponse;
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
}

/// Peers allowed to scrape `/metrics`: loopback and private-range
/// (RFC 1918 / IPv6 ULA) addresses — the only sources the in-cluster
/// Prometheus scraper and host-local tooling ever connect from. Public peers
/// are refused so a directly-exposed process never leaks Prometheus internals
/// even if the reverse proxy's own `/metrics` block is missing.
fn is_trusted_metrics_peer(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_loopback() || v4.is_private() || v4.is_link_local(),
        // fc00::/7 (unique local) plus loopback; `is_unique_local` is unstable.
        IpAddr::V6(v6) => v6.is_loopback() || (v6.segments()[0] & 0xfe00) == 0xfc00,
    }
}

/// Build the complete application router. `metrics` is None in tests (no
/// global Prometheus recorder there).
pub fn build_router(state: AppState, metrics: Option<PrometheusHandle>) -> Router {
    let config = Arc::clone(&state.config);

    // ---- documented business routes (absolute /api/v1/... paths) ----
    // Auth is split out so it can carry its own, much stricter rate limit:
    // login/refresh/logout are the only unauthenticated business endpoints.
    let (api, mut api_doc) = OpenApiRouter::with_openapi(openapi::ApiDoc::openapi())
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
    // Default-on authentication: every route above requires a valid bearer
    // token via this router-level guard — a handler cannot opt out by
    // forgetting an AuthUser parameter. Only auth_api below (login/refresh/
    // logout), the HMAC-signed storage routes, and ops endpoints are public.
    let api = api.route_layer(from_fn_with_state(state.clone(), auth::require_auth));
    let (auth_api, auth_doc) = OpenApiRouter::new()
        .merge(auth::handlers::router())
        .split_for_parts();
    api_doc.merge(auth_doc);

    // Per-client rate limiting on business routes only (never health
    // checks): a JWT-protected flood ceiling on the api, a slow bucket on
    // the public auth endpoints.
    let (api, auth_api) = if config.rate_limit_enabled {
        let make_governor = |per_second: u64, burst: u32| {
            let governor_config = Arc::new(
                GovernorConfigBuilder::default()
                    .per_second(per_second)
                    .burst_size(burst)
                    .key_extractor(ClientIpKeyExtractor)
                    .finish()
                    .expect("governor configuration is validated at boot"),
            );
            // Periodically evict idle per-client buckets so the limiter's
            // memory stays bounded.
            let limiter = governor_config.limiter().clone();
            tokio::spawn(async move {
                let mut ticker = tokio::time::interval(Duration::from_secs(60));
                loop {
                    ticker.tick().await;
                    limiter.retain_recent();
                }
            });
            GovernorLayer::new(governor_config).error_handler(rate_limit_error)
        };
        (
            api.layer(make_governor(
                config.rate_limit_per_second,
                config.rate_limit_burst,
            )),
            auth_api.layer(make_governor(
                config.rate_limit_auth_per_second,
                config.rate_limit_auth_burst,
            )),
        )
    } else {
        (api, auth_api)
    };
    let api = api.merge(auth_api);

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
            get(move |ConnectInfo(peer): ConnectInfo<SocketAddr>| {
                let handle = handle.clone();
                async move {
                    // Defense in depth: the reverse proxy already blocks public
                    // /metrics, but never hand Prometheus internals to a public
                    // peer even if that block is ever missing. The scraper and
                    // host tooling reach us over loopback/private addresses;
                    // anything else gets the same 404 as an unknown route.
                    if is_trusted_metrics_peer(peer.ip()) {
                        handle.render().into_response()
                    } else {
                        ops::not_found().await
                    }
                }
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
