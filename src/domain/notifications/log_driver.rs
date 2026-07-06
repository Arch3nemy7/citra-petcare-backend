//! Development driver: notifications are written to the log instead of being
//! pushed anywhere.

use async_trait::async_trait;

use super::{Notifier, NotifyError, OutboundMessage};

pub struct LogNotifier;

#[async_trait]
impl Notifier for LogNotifier {
    async fn send(&self, message: &OutboundMessage) -> Result<(), NotifyError> {
        tracing::info!(
            title = %message.title,
            body = %message.body,
            data = ?message.data,
            "notification dispatched (log driver)"
        );
        Ok(())
    }

    fn name(&self) -> &'static str {
        "log"
    }
}
