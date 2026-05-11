use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use crate::outbox::{DeliveryJob, DeliveryOutbox, DeliveryOutboxError};
use crate::retry::RetryPolicy;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationMode {
    Disabled,
    LocalMock,
    LocalReal,
    Provider,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum NotificationChannel {
    Email,
    Sms,
    Push,
    InApp,
    Custom(String),
}

impl NotificationChannel {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Email => "email",
            Self::Sms => "sms",
            Self::Push => "push",
            Self::InApp => "in_app",
            Self::Custom(value) => value.as_str(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NotificationMessage {
    pub id: Uuid,
    pub tenant_id: String,
    pub recipient: String,
    pub channel: NotificationChannel,
    pub template: String,
    pub payload: serde_json::Value,
}

impl NotificationMessage {
    pub fn new(
        tenant_id: impl Into<String>,
        recipient: impl Into<String>,
        channel: NotificationChannel,
        template: impl Into<String>,
        payload: serde_json::Value,
    ) -> Self {
        Self {
            id: Uuid::now_v7(),
            tenant_id: tenant_id.into(),
            recipient: recipient.into(),
            channel,
            template: template.into(),
            payload,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationStatus {
    Queued,
    SkippedDisabled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationOutcome {
    pub message_id: Uuid,
    pub status: NotificationStatus,
}

#[derive(Debug, thiserror::Error)]
pub enum NotificationError {
    #[error("notification delivery disabled")]
    Disabled,
    #[error(transparent)]
    Outbox(#[from] DeliveryOutboxError),
}

#[async_trait]
pub trait NotificationPort: Send + Sync {
    async fn send(
        &self,
        message: NotificationMessage,
    ) -> Result<NotificationOutcome, NotificationError>;
}

#[derive(Debug, Default)]
pub struct NoopNotificationPort;

#[async_trait]
impl NotificationPort for NoopNotificationPort {
    async fn send(
        &self,
        message: NotificationMessage,
    ) -> Result<NotificationOutcome, NotificationError> {
        Ok(NotificationOutcome {
            message_id: message.id,
            status: NotificationStatus::SkippedDisabled,
        })
    }
}

pub struct OutboxNotificationPort {
    outbox: Arc<dyn DeliveryOutbox>,
    retry_policy: RetryPolicy,
}

impl OutboxNotificationPort {
    pub fn new(outbox: Arc<dyn DeliveryOutbox>, retry_policy: RetryPolicy) -> Self {
        Self {
            outbox,
            retry_policy,
        }
    }
}

#[async_trait]
impl NotificationPort for OutboxNotificationPort {
    async fn send(
        &self,
        message: NotificationMessage,
    ) -> Result<NotificationOutcome, NotificationError> {
        let payload = serde_json::json!({
            "message_id": message.id,
            "template": message.template,
            "payload": message.payload,
        });
        self.outbox
            .enqueue(DeliveryJob::notification(
                message.tenant_id,
                message.channel.as_str(),
                message.recipient,
                payload,
                self.retry_policy,
            ))
            .await?;
        Ok(NotificationOutcome {
            message_id: message.id,
            status: NotificationStatus::Queued,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::outbox::InMemoryDeliveryOutbox;

    #[tokio::test]
    async fn noop_notification_mode_skips_without_enqueueing() {
        let port = NoopNotificationPort;
        let message = NotificationMessage::new(
            "tenant-a",
            "user@example.test",
            NotificationChannel::Email,
            "welcome",
            serde_json::json!({ "name": "Ada" }),
        );

        let outcome = port.send(message).await.unwrap();

        assert_eq!(outcome.status, NotificationStatus::SkippedDisabled);
    }

    #[tokio::test]
    async fn outbox_notification_mode_queues_delivery_job() {
        let outbox = Arc::new(InMemoryDeliveryOutbox::default());
        let port = OutboxNotificationPort::new(outbox.clone(), RetryPolicy::no_retry());
        let message = NotificationMessage::new(
            "tenant-a",
            "user@example.test",
            NotificationChannel::Email,
            "welcome",
            serde_json::json!({ "name": "Ada" }),
        );

        let outcome = port.send(message).await.unwrap();
        let due = outbox.claim_due(10, chrono::Utc::now()).await.unwrap();

        assert_eq!(outcome.status, NotificationStatus::Queued);
        assert_eq!(due.len(), 1);
    }
}
