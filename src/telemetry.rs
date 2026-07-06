//! Logging (tracing) and metrics (Prometheus) initialization.

use std::time::Duration;

use metrics_exporter_prometheus::{Matcher, PrometheusBuilder, PrometheusHandle};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, fmt};

use crate::config::{Config, LogFormat};

/// Install the global tracing subscriber. JSON output in production so logs
/// are machine-parseable; human-readable output for local development.
pub fn init_tracing(config: &Config) {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,citra_petcare=debug,tower_http=info,sqlx=warn"));

    match config.log_format {
        LogFormat::Json => {
            tracing_subscriber::registry()
                .with(filter)
                .with(
                    fmt::layer()
                        .json()
                        .with_current_span(true)
                        .with_span_list(false),
                )
                .init();
        }
        LogFormat::Pretty => {
            tracing_subscriber::registry()
                .with(filter)
                .with(fmt::layer())
                .init();
        }
    }
}

/// Latency buckets tuned for a fast local API: 0.5 ms – 10 s.
const HTTP_LATENCY_BUCKETS: &[f64] = &[
    0.0005, 0.001, 0.0025, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
];

/// Install the Prometheus metrics recorder and return the handle used by the
/// /metrics endpoint to render the current snapshot. Call once per process.
pub fn init_metrics() -> PrometheusHandle {
    let handle = PrometheusBuilder::new()
        .set_buckets_for_metric(
            Matcher::Full("http_request_duration_seconds".to_string()),
            HTTP_LATENCY_BUCKETS,
        )
        .expect("bucket list is non-empty")
        .install_recorder()
        .expect("metrics recorder is installed once per process");

    metrics::describe_histogram!(
        "http_request_duration_seconds",
        metrics::Unit::Seconds,
        "HTTP request latency by method/route/status"
    );
    metrics::describe_counter!(
        "http_requests_total",
        "HTTP requests by method/route/status"
    );

    // The prometheus recorder buffers histogram samples internally; a periodic
    // upkeep pass drains them so memory stays bounded between scrapes.
    let upkeep = handle.clone();
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(5));
        loop {
            ticker.tick().await;
            upkeep.run_upkeep();
        }
    });

    handle
}
