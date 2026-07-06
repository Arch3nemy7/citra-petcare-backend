use std::sync::Arc;

use sqlx::PgPool;

use crate::config::Config;
use crate::domain::notifications::Notifier;
use crate::domain::storage::Storage;

/// Shared application state, injected into handlers with axum's `State`.
///
/// Cloning is cheap: `PgPool` is internally reference-counted and the other
/// fields are `Arc`s. `dyn Storage` / `dyn Notifier` are trait objects so dev
/// and tests can swap drivers (local disk vs S3, log vs FCM) without generic
/// parameters rippling through every handler signature.
#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub config: Arc<Config>,
    pub storage: Arc<dyn Storage>,
    pub notifier: Arc<dyn Notifier>,
}
