pub mod dto;
pub mod fcm;
pub mod handlers;
pub mod log_driver;
pub mod models;
pub mod repo;
pub mod service;

use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;

use crate::config::{Config, NotifierConfig};

#[derive(Debug, thiserror::Error)]
pub enum NotifyError {
    #[error("notifier configuration error: {0}")]
    Config(String),
    #[error("notification backend error: {0}")]
    Backend(String),
}

/// Outbound push message, independent of the delivery mechanism.
#[derive(Debug, Clone)]
pub struct OutboundMessage {
    pub title: String,
    pub body: String,
    /// Key/value payload. FCM only allows string values, so we standardize on
    /// that for every driver.
    pub data: BTreeMap<String, String>,
}

/// Push-notification channel. `async_trait` because native async-fn-in-trait
/// is not yet object-safe, and AppState holds this as `Arc<dyn Notifier>`.
#[async_trait]
pub trait Notifier: Send + Sync {
    async fn send(&self, message: &OutboundMessage) -> Result<(), NotifyError>;
    /// Driver name for logs and diagnostics.
    fn name(&self) -> &'static str;
}

/// Construct the configured driver at boot.
pub async fn build(config: &Config) -> Result<Arc<dyn Notifier>, NotifyError> {
    match &config.notifier {
        NotifierConfig::Log => Ok(Arc::new(log_driver::LogNotifier)),
        NotifierConfig::Fcm {
            service_account_path,
            topic,
        } => Ok(Arc::new(
            fcm::FcmNotifier::new(service_account_path, topic.clone()).await?,
        )),
    }
}
