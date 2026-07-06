//! Firebase Cloud Messaging driver, speaking the FCM HTTP v1 API directly:
//! OAuth2 service-account token via gcp_auth, then a plain JSON POST to
//! `projects/{project}/messages:send`. Both clinic phones subscribe to one
//! FCM topic, so messages are addressed to the topic rather than to
//! individual device tokens.

use std::path::Path;
use std::time::Duration;

use async_trait::async_trait;
use gcp_auth::{CustomServiceAccount, TokenProvider};

use super::{Notifier, NotifyError, OutboundMessage};

const FCM_SCOPE: &str = "https://www.googleapis.com/auth/firebase.messaging";

pub struct FcmNotifier {
    http: reqwest::Client,
    auth: CustomServiceAccount,
    endpoint: String,
    topic: String,
}

impl FcmNotifier {
    pub async fn new(service_account_path: &Path, topic: String) -> Result<Self, NotifyError> {
        let auth = CustomServiceAccount::from_file(service_account_path)
            .map_err(|e| NotifyError::Config(format!("cannot load FCM service account: {e}")))?;
        let project_id = auth
            .project_id()
            .ok_or_else(|| {
                NotifyError::Config("service account JSON is missing project_id".to_string())
            })?
            .to_string();
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .map_err(|e| NotifyError::Config(format!("failed to build HTTP client: {e}")))?;
        Ok(Self {
            http,
            auth,
            endpoint: format!("https://fcm.googleapis.com/v1/projects/{project_id}/messages:send"),
            topic,
        })
    }
}

#[async_trait]
impl Notifier for FcmNotifier {
    async fn send(&self, message: &OutboundMessage) -> Result<(), NotifyError> {
        // gcp_auth caches the token internally and refreshes it when expired
        let token = self
            .auth
            .token(&[FCM_SCOPE])
            .await
            .map_err(|e| NotifyError::Backend(format!("FCM auth failed: {e}")))?;

        let payload = serde_json::json!({
            "message": {
                "topic": self.topic,
                "notification": { "title": message.title, "body": message.body },
                "data": message.data,
            }
        });

        let response = self
            .http
            .post(&self.endpoint)
            .bearer_auth(token.as_str())
            .json(&payload)
            .send()
            .await
            .map_err(|e| NotifyError::Backend(format!("FCM request failed: {e}")))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(NotifyError::Backend(format!(
                "FCM returned {status}: {body}"
            )));
        }
        Ok(())
    }

    fn name(&self) -> &'static str {
        "fcm"
    }
}
