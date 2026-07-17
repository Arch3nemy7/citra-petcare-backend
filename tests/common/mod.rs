//! Shared integration-test harness: a real Postgres 16 in a testcontainer,
//! migrations applied via the embedded migrator, and the production router
//! exercised in-process through `tower::ServiceExt::oneshot` — no sockets.
//!
//! Tests deliberately use runtime-checked `sqlx::query()` (not the compile
//! time macros) so `cargo sqlx prepare` only has to cover the library crate.

// Each test binary compiles this module separately and uses only a subset of
// the helpers, so "unused" warnings here are noise.
#![allow(dead_code)]

use std::net::IpAddr;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use citra_petcare::config::{AppEnv, Config, LogFormat, NotifierConfig, StorageConfig};
use citra_petcare::db::MIGRATOR;
use citra_petcare::domain::auth::password;
use citra_petcare::domain::notifications::log_driver::LogNotifier;
use citra_petcare::domain::storage::local::LocalStorage;
use citra_petcare::http::build_router;
use citra_petcare::state::AppState;
use http_body_util::BodyExt;
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::testcontainers::{ContainerAsync, ImageExt};
use tower::ServiceExt;
use uuid::Uuid;

pub const TEST_PASSWORD: &str = "correct-horse-battery";

pub struct TestApp {
    pub router: Router,
    pub db: PgPool,
    /// Dropping the container kills the database — keep it alive with the app.
    _container: ContainerAsync<Postgres>,
}

pub async fn spawn_app() -> TestApp {
    let container = Postgres::default()
        .with_tag("16-alpine")
        .start()
        .await
        .expect("postgres testcontainer starts (is Docker running?)");
    let port = container
        .get_host_port_ipv4(5432)
        .await
        .expect("mapped 5432");
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");

    let db = PgPoolOptions::new()
        .max_connections(5)
        .connect(&url)
        .await
        .expect("connect to test postgres");
    MIGRATOR.run(&db).await.expect("migrations apply cleanly");

    let storage_root = std::env::temp_dir().join(format!("petcare-test-{}", Uuid::now_v7()));
    let config = test_config(url, storage_root.clone());
    let state = AppState {
        db: db.clone(),
        config: Arc::new(config),
        storage: Arc::new(LocalStorage::new(
            storage_root,
            "integration-test-secret-0123456789abcdef".to_string(),
            "http://localhost:8080".to_string(),
        )),
        notifier: Arc::new(LogNotifier),
    };

    TestApp {
        router: build_router(state, None),
        db,
        _container: container,
    }
}

fn test_config(database_url: String, storage_root: std::path::PathBuf) -> Config {
    Config {
        app_env: AppEnv::Development,
        http_host: IpAddr::from([127, 0, 0, 1]),
        http_port: 0,
        log_format: LogFormat::Pretty,
        public_base_url: "http://localhost:8080".to_string(),
        database_url,
        database_max_connections: 5,
        auto_migrate: false,
        jwt_secret: "integration-test-secret-0123456789abcdef".to_string(),
        access_token_ttl_secs: 900,
        refresh_token_ttl_days: 30,
        cors_allowed_origins: vec![],
        // oneshot() requests carry no client address, so the IP rate limiter
        // cannot extract a key — disabled in tests.
        rate_limit_enabled: false,
        rate_limit_per_second: 100,
        rate_limit_burst: 200,
        rate_limit_auth_per_second: 100,
        rate_limit_auth_burst: 200,
        request_timeout_secs: 30,
        body_limit_bytes: 2 * 1024 * 1024,
        presign_ttl_secs: 900,
        storage: StorageConfig::Local { root: storage_root },
        notifier: NotifierConfig::Log,
        scheduler_enabled: false,
    }
}

/// Insert a staff user directly (there is no public registration).
pub async fn create_user(db: &PgPool, email: &str) -> Uuid {
    let id = Uuid::now_v7();
    let hash = password::hash_password(TEST_PASSWORD.to_string())
        .await
        .expect("hashes");
    sqlx::query(
        "INSERT INTO users (id, name, email, password_hash, role) VALUES ($1, $2, $3, $4, 'VET')",
    )
    .bind(id)
    .bind("drh. Test")
    .bind(email)
    .bind(hash)
    .execute(db)
    .await
    .expect("test user inserts");
    id
}

/// Fire one request at the router and return (status, parsed JSON body).
/// Non-JSON/empty bodies come back as `Value::Null`.
pub async fn request(
    router: &Router,
    method: &str,
    uri: &str,
    token: Option<&str>,
    body: Option<serde_json::Value>,
) -> (StatusCode, serde_json::Value) {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(token) = token {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    let request = match body {
        Some(json) => builder
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(json.to_string()))
            .expect("request builds"),
        None => builder.body(Body::empty()).expect("request builds"),
    };

    let response = router
        .clone()
        .oneshot(request)
        .await
        .expect("router responds");
    let status = response.status();
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body reads")
        .to_bytes();
    let json = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
    (status, json)
}

/// Like [`request`], but with a raw `Authorization` value (scheme included),
/// for tests that exercise header parsing itself.
pub async fn request_with_authorization(
    router: &Router,
    method: &str,
    uri: &str,
    authorization: &str,
) -> (StatusCode, serde_json::Value) {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::AUTHORIZATION, authorization)
        .body(Body::empty())
        .expect("request builds");
    let response = router
        .clone()
        .oneshot(request)
        .await
        .expect("router responds");
    let status = response.status();
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body reads")
        .to_bytes();
    let json = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
    (status, json)
}

/// Login and return (access_token, refresh_token).
pub async fn login(router: &Router, email: &str) -> (String, String) {
    let (status, body) = request(
        router,
        "POST",
        "/api/v1/auth/login",
        None,
        Some(serde_json::json!({ "email": email, "password": TEST_PASSWORD })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "login failed: {body}");
    (
        body["accessToken"]
            .as_str()
            .expect("accessToken present")
            .to_string(),
        body["refreshToken"]
            .as_str()
            .expect("refreshToken present")
            .to_string(),
    )
}

/// Convenience: fresh app + one user + tokens.
pub async fn spawn_logged_in() -> (TestApp, String) {
    let app = spawn_app().await;
    create_user(&app.db, "vet@test.id").await;
    let (access, _) = login(&app.router, "vet@test.id").await;
    (app, access)
}

/// Create an owner via the API, returning its id.
pub async fn create_owner(router: &Router, token: &str, name: &str) -> Uuid {
    let (status, body) = request(
        router,
        "POST",
        "/api/v1/owners",
        Some(token),
        Some(serde_json::json!({ "name": name, "phone": "+628123456789" })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "owner create failed: {body}");
    body["id"].as_str().unwrap().parse().unwrap()
}

/// Create a patient via the API, returning its id.
pub async fn create_patient(router: &Router, token: &str, owner_id: Uuid, name: &str) -> Uuid {
    let (status, body) = request(
        router,
        "POST",
        "/api/v1/patients",
        Some(token),
        Some(serde_json::json!({
            "ownerId": owner_id,
            "name": name,
            "species": "CAT",
            "sex": "FEMALE",
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "patient create failed: {body}");
    body["id"].as_str().unwrap().parse().unwrap()
}
