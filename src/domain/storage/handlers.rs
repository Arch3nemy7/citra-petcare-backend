use std::time::Duration;

use axum::Json;
use axum::body::Body;
use axum::extract::{DefaultBodyLimit, Path, Query, State};
use axum::http::{StatusCode, header};
use axum::response::Response;
use axum::routing::{get, put};
use chrono::Utc;
use serde::Deserialize;
use tokio_util::io::ReaderStream;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use super::dto::{PresignResponse, PresignUploadRequest};
use super::local::LocalStorage;
use super::validate_key;
use crate::config::StorageConfig;
use crate::domain::auth::AuthUser;
use crate::error::AppError;
use crate::http::extract::ValidatedJson;
use crate::http::problem::ProblemDetails;
use crate::state::AppState;

/// Uploads through the local dev driver can be larger than the JSON body
/// limit: photos and x-ray scans.
const LOCAL_UPLOAD_LIMIT_BYTES: usize = 50 * 1024 * 1024;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(presign_upload))
        // Wildcard route registered manually: axum needs `{*key}` to match
        // keys containing slashes, which utoipa cannot express — the operation
        // is documented separately in http::openapi with a plain {key} param.
        .route(
            "/api/v1/storage/presign-download/{*key}",
            get(presign_download),
        )
}

/// Undocumented dev-only routes backing the local driver's "presigned" URLs.
/// Auth is the HMAC signature in the URL, exactly like real presigned URLs.
pub fn local_router() -> axum::Router<AppState> {
    axum::Router::new()
        .route(
            "/api/v1/storage/local/{*key}",
            put(local_upload).get(local_download),
        )
        .layer(DefaultBodyLimit::max(LOCAL_UPLOAD_LIMIT_BYTES))
}

/// Get a presigned URL for uploading a file. The client PUTs the file bytes
/// to `url` with the returned headers, then stores `key` on the entity
/// (patient photo, visit attachment, …).
#[utoipa::path(
    post,
    path = "/api/v1/storage/presign-upload",
    tag = "storage",
    request_body = PresignUploadRequest,
    responses(
        (status = 200, description = "Presigned upload", body = PresignResponse),
        (status = 401, description = "Not authenticated", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 422, description = "Unsupported content type", body = ProblemDetails, content_type = "application/problem+json"),
    ),
    security(("bearerAuth" = []))
)]
pub async fn presign_upload(
    State(state): State<AppState>,
    _user: AuthUser,
    ValidatedJson(body): ValidatedJson<PresignUploadRequest>,
) -> Result<Json<PresignResponse>, AppError> {
    let key = generate_key(&body.file_name);
    let ttl = Duration::from_secs(state.config.presign_ttl_secs);
    let presigned = state
        .storage
        .presign_upload(&key, &body.content_type, ttl)
        .await?;
    Ok(Json(PresignResponse::from_presigned(Some(key), presigned)))
}

/// Get a presigned URL for downloading a stored file.
///
/// Documented in the OpenAPI spec via `http::openapi` (see `router()` above).
#[utoipa::path(
    get,
    path = "/api/v1/storage/presign-download/{key}",
    tag = "storage",
    params(("key" = String, Path, description = "Storage key, may contain slashes", example = "uploads/2026/07/0197xyz-rontgen-bruno.jpg")),
    responses(
        (status = 200, description = "Presigned download", body = PresignResponse),
        (status = 400, description = "Invalid key", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 401, description = "Not authenticated", body = ProblemDetails, content_type = "application/problem+json"),
    ),
    security(("bearerAuth" = []))
)]
pub async fn presign_download(
    State(state): State<AppState>,
    _user: AuthUser,
    Path(key): Path<String>,
) -> Result<Json<PresignResponse>, AppError> {
    validate_key(&key)?;
    let ttl = Duration::from_secs(state.config.presign_ttl_secs);
    let presigned = state.storage.presign_download(&key, ttl).await?;
    Ok(Json(PresignResponse::from_presigned(None, presigned)))
}

/// `uploads/YYYY/MM/{uuidv7}-{sanitized-name}` — unique, sortable, readable.
fn generate_key(file_name: &str) -> String {
    let sanitized: String = file_name
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '-'
            }
        })
        .take(80)
        .collect();
    format!(
        "uploads/{}/{}-{}",
        Utc::now().format("%Y/%m"),
        Uuid::now_v7(),
        sanitized
    )
}

#[derive(Debug, Deserialize)]
struct LocalSigQuery {
    exp: i64,
    sig: String,
}

fn local_storage(state: &AppState) -> Result<LocalStorage, AppError> {
    match &state.config.storage {
        StorageConfig::Local { root } => Ok(LocalStorage::new(
            root.clone(),
            state.config.jwt_secret.clone(),
            state.config.public_base_url.clone(),
        )),
        _ => Err(AppError::NotFound("route")),
    }
}

async fn local_upload(
    State(state): State<AppState>,
    Path(key): Path<String>,
    Query(query): Query<LocalSigQuery>,
    body: axum::body::Bytes,
) -> Result<StatusCode, AppError> {
    let storage = local_storage(&state)?;
    storage.verify("PUT", &key, query.exp, &query.sig)?;
    storage.save(&key, &body).await?;
    Ok(StatusCode::CREATED)
}

async fn local_download(
    State(state): State<AppState>,
    Path(key): Path<String>,
    Query(query): Query<LocalSigQuery>,
) -> Result<Response, AppError> {
    let storage = local_storage(&state)?;
    storage.verify("GET", &key, query.exp, &query.sig)?;
    let file = storage.open(&key).await?;
    // Stream from disk instead of buffering whole files in memory.
    let stream = ReaderStream::new(file);
    let response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .body(Body::from_stream(stream))
        .map_err(|e| AppError::Internal(format!("failed to build response: {e}")))?;
    Ok(response)
}
