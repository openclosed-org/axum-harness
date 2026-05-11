use async_trait::async_trait;
use chrono::{Duration as ChronoDuration, Utc};
use std::collections::BTreeMap;
use std::sync::Arc;

use crate::outbox::{DeliveryJob, DeliveryOutbox, DeliveryOutboxError, DeliveryTarget};
use crate::retry::RetryPolicy;
use crate::webhook::HmacSha256WebhookSigner;

#[derive(Debug, thiserror::Error)]
pub enum DeliveryError {
    #[error("delivery transport failed: {0}")]
    Transport(String),
    #[error(transparent)]
    Outbox(#[from] DeliveryOutboxError),
    #[error("delivery payload serialization failed: {0}")]
    Serialization(String),
}

#[async_trait]
pub trait OutboundTransport: Send + Sync {
    async fn deliver(
        &self,
        job: &DeliveryJob,
        headers: &BTreeMap<String, String>,
        body: &[u8],
    ) -> Result<(), DeliveryError>;
}

#[derive(Debug, Default)]
pub struct NoopOutboundTransport;

#[async_trait]
impl OutboundTransport for NoopOutboundTransport {
    async fn deliver(
        &self,
        _job: &DeliveryJob,
        _headers: &BTreeMap<String, String>,
        _body: &[u8],
    ) -> Result<(), DeliveryError> {
        Ok(())
    }
}

pub struct DeliveryWorker {
    outbox: Arc<dyn DeliveryOutbox>,
    transport: Arc<dyn OutboundTransport>,
    retry_policy: RetryPolicy,
}

impl DeliveryWorker {
    pub fn new(
        outbox: Arc<dyn DeliveryOutbox>,
        transport: Arc<dyn OutboundTransport>,
        retry_policy: RetryPolicy,
    ) -> Self {
        Self {
            outbox,
            transport,
            retry_policy,
        }
    }

    pub async fn deliver_due(&self, limit: usize) -> Result<usize, DeliveryError> {
        let jobs = self.outbox.claim_due(limit, Utc::now()).await?;
        let mut delivered = 0;

        for job in jobs {
            let attempted = self.outbox.record_attempt(job.id).await?;
            let body = serde_json::to_vec(&attempted.payload)
                .map_err(|error| DeliveryError::Serialization(error.to_string()))?;
            let headers = signed_headers(&attempted, &body);

            match self.transport.deliver(&attempted, &headers, &body).await {
                Ok(()) => {
                    self.outbox.mark_delivered(attempted.id).await?;
                    delivered += 1;
                }
                Err(error)
                    if self
                        .retry_policy
                        .should_retry_after(attempted.attempt_count) =>
                {
                    let delay = self.retry_policy.next_delay(attempted.attempt_count);
                    let chrono_delay = ChronoDuration::from_std(delay)
                        .map_err(|err| DeliveryError::Transport(err.to_string()))?;
                    self.outbox
                        .mark_retry(attempted.id, error.to_string(), Utc::now() + chrono_delay)
                        .await?;
                }
                Err(error) => {
                    self.outbox
                        .mark_dead_lettered(attempted.id, error.to_string())
                        .await?;
                }
            }
        }

        Ok(delivered)
    }
}

fn signed_headers(job: &DeliveryJob, body: &[u8]) -> BTreeMap<String, String> {
    let mut headers = job.headers.clone();
    if let DeliveryTarget::Webhook {
        signing_secret: Some(secret),
        ..
    } = &job.target
    {
        headers.insert(
            HmacSha256WebhookSigner::HEADER.to_string(),
            HmacSha256WebhookSigner::sign(secret, body),
        );
    }
    headers
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::outbox::{DeliveryJobStatus, InMemoryDeliveryOutbox};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    struct FailsBeforeSuccess {
        failures_remaining: AtomicUsize,
    }

    #[async_trait]
    impl OutboundTransport for FailsBeforeSuccess {
        async fn deliver(
            &self,
            _job: &DeliveryJob,
            _headers: &BTreeMap<String, String>,
            _body: &[u8],
        ) -> Result<(), DeliveryError> {
            let previous =
                self.failures_remaining
                    .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |value| {
                        value.checked_sub(1)
                    });
            if previous.is_ok() {
                return Err(DeliveryError::Transport("temporary failure".to_string()));
            }
            Ok(())
        }
    }

    #[tokio::test]
    async fn worker_retries_failed_delivery_then_succeeds() {
        let outbox = Arc::new(InMemoryDeliveryOutbox::default());
        let retry_policy = RetryPolicy::new(3, Duration::from_millis(0));
        let job = outbox
            .enqueue(DeliveryJob::notification(
                "tenant-a",
                "email",
                "user@example.test",
                serde_json::json!({ "hello": "world" }),
                retry_policy,
            ))
            .await
            .unwrap();
        let worker = DeliveryWorker::new(
            outbox.clone(),
            Arc::new(FailsBeforeSuccess {
                failures_remaining: AtomicUsize::new(1),
            }),
            retry_policy,
        );

        assert_eq!(worker.deliver_due(10).await.unwrap(), 0);
        let failed = outbox.get(job.id).await.unwrap().unwrap();
        assert_eq!(failed.status, DeliveryJobStatus::Failed);
        assert_eq!(failed.attempt_count, 1);

        assert_eq!(worker.deliver_due(10).await.unwrap(), 1);
        let delivered = outbox.get(job.id).await.unwrap().unwrap();
        assert_eq!(delivered.status, DeliveryJobStatus::Delivered);
        assert_eq!(delivered.attempt_count, 2);
    }

    #[tokio::test]
    async fn worker_dead_letters_after_retry_budget_is_exhausted() {
        let outbox = Arc::new(InMemoryDeliveryOutbox::default());
        let retry_policy = RetryPolicy::new(2, Duration::from_millis(0));
        let job = outbox
            .enqueue(DeliveryJob::notification(
                "tenant-a",
                "email",
                "user@example.test",
                serde_json::json!({ "hello": "world" }),
                retry_policy,
            ))
            .await
            .unwrap();
        let worker = DeliveryWorker::new(
            outbox.clone(),
            Arc::new(FailsBeforeSuccess {
                failures_remaining: AtomicUsize::new(10),
            }),
            retry_policy,
        );

        worker.deliver_due(10).await.unwrap();
        worker.deliver_due(10).await.unwrap();

        let dead = outbox.get(job.id).await.unwrap().unwrap();
        assert_eq!(dead.status, DeliveryJobStatus::DeadLettered);
        assert_eq!(outbox.dead_letters().await.unwrap().len(), 1);
    }

    struct CapturesHeaders {
        signature_seen: tokio::sync::Mutex<Option<String>>,
    }

    #[async_trait]
    impl OutboundTransport for CapturesHeaders {
        async fn deliver(
            &self,
            _job: &DeliveryJob,
            headers: &BTreeMap<String, String>,
            _body: &[u8],
        ) -> Result<(), DeliveryError> {
            *self.signature_seen.lock().await = headers
                .get(HmacSha256WebhookSigner::HEADER)
                .map(std::string::ToString::to_string);
            Ok(())
        }
    }

    #[tokio::test]
    async fn worker_signs_outbound_webhook_deliveries() {
        let outbox = Arc::new(InMemoryDeliveryOutbox::default());
        let retry_policy = RetryPolicy::no_retry();
        outbox
            .enqueue(DeliveryJob::webhook(
                "tenant-a",
                "endpoint-a",
                "https://example.test/webhook",
                Some("secret".to_string()),
                serde_json::json!({ "value": 1 }),
                BTreeMap::new(),
                retry_policy,
            ))
            .await
            .unwrap();
        let transport = Arc::new(CapturesHeaders {
            signature_seen: tokio::sync::Mutex::new(None),
        });
        let worker = DeliveryWorker::new(outbox, transport.clone(), retry_policy);

        worker.deliver_due(10).await.unwrap();

        assert!(transport.signature_seen.lock().await.is_some());
    }
}
