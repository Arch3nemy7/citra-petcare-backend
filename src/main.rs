//! Binary entrypoint — wiring only. All behavior lives in the library crate
//! so integration tests exercise the identical router and services.

use std::net::SocketAddr;
use std::sync::Arc;

use clap::{Parser, Subcommand};
use tokio::net::TcpListener;
use tokio::signal;

use citra_petcare::config::Config;
use citra_petcare::state::AppState;
use citra_petcare::{db, domain, http, scheduler, seed, telemetry};

#[derive(Parser)]
#[command(name = "citra-petcare", version, about = "Citra PetCare clinic API")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Run the HTTP API server (default when no subcommand is given)
    Serve,
    /// Apply pending database migrations, then exit
    Migrate,
    /// Apply migrations, then insert demo data (2 vets + sample clinic data)
    Seed,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok(); // .env is optional; real environment always wins

    let cli = Cli::parse();
    let config = match Config::from_env() {
        Ok(config) => config,
        Err(error) => {
            // tracing is not up yet — print the full problem list and exit
            eprintln!("{error}");
            std::process::exit(2);
        }
    };
    telemetry::init_tracing(&config);

    // Several dependencies link rustls with different crypto backends; pin
    // one process-wide default so none of them panics on first TLS use.
    rustls::crypto::ring::default_provider()
        .install_default()
        .ok();

    let pool = db::connect(&config).await?;

    match cli.command.unwrap_or(Command::Serve) {
        Command::Migrate => {
            db::migrate(&pool).await?;
            tracing::info!("migrations applied");
        }
        Command::Seed => {
            db::migrate(&pool).await?;
            seed::run(&pool)
                .await
                .map_err(|e| anyhow::anyhow!("seed failed: {e}"))?;
        }
        Command::Serve => serve(config, pool).await?,
    }
    Ok(())
}

async fn serve(config: Config, pool: sqlx::PgPool) -> anyhow::Result<()> {
    if config.auto_migrate {
        db::migrate(&pool).await?;
        tracing::info!("migrations applied (AUTO_MIGRATE=true)");
    }

    let storage = domain::storage::build(&config).await?;
    let notifier = domain::notifications::build(&config).await?;
    tracing::info!(
        storage = storage.name(),
        notifier = notifier.name(),
        "drivers ready"
    );

    let state = AppState {
        db: pool.clone(),
        config: Arc::new(config.clone()),
        storage,
        notifier,
    };

    let job_scheduler = if config.scheduler_enabled {
        Some(scheduler::start(state.clone()).await?)
    } else {
        tracing::info!("scheduler disabled (SCHEDULER_ENABLED=false)");
        None
    };

    let metrics = telemetry::init_metrics();
    let app = http::build_router(state, Some(metrics));

    let addr = SocketAddr::new(config.http_host, config.http_port);
    let listener = TcpListener::bind(addr).await?;
    tracing::info!(%addr, docs = "/docs", "citra-petcare API listening");

    // ConnectInfo lets the rate limiter fall back to the socket peer address
    // when no reverse proxy provides X-Forwarded-For.
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await?;

    if let Some(mut job_scheduler) = job_scheduler {
        job_scheduler.shutdown().await.ok();
    }
    pool.close().await;
    tracing::info!("shutdown complete");
    Ok(())
}

/// Resolves on SIGINT (Ctrl-C) or SIGTERM (docker stop, systemd).
async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c().await.expect("ctrl-c handler installs");
    };
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("SIGTERM handler installs")
            .recv()
            .await;
    };
    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    tracing::info!("shutdown signal received; draining connections");
}
