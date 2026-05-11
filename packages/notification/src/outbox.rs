use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use uuid::Uuid;

use crate::retry::RetryPolicy;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeliveryKind {
    Notification,
    OutboundWebhook,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeliveryTarget {
    Notification {
        channel: String,
        recipient: String,
    },
    Webhook {
        endpoint_id: String,
        url: String,
        signing_secret: Option<String>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeliveryJobStatus {
    Pending,
    InProgress,
    Delivered,
    Failed,
    DeadLettered,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeliveryJob {
    pub id: Uuid,
    pub tenant_id: String,
    pub kind: DeliveryKind,
    pub target: DeliveryTarget,
    pub payload: serde_json::Value,
    pub headers: BTreeMap<String, String>,
    pub status: DeliveryJobStatus,
    pub attempt_count: u32,
    pub max_attempts: u32,
    pub available_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub last_error: Option<String>,
}

impl DeliveryJob {
    pub fn notification(
        tenant_id: impl Into<String>,
        channel: impl Into<String>,
        recipient: impl Into<String>,
        payload: serde_json::Value,
        retry_policy: RetryPolicy,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::now_v7(),
            tenant_id: tenant_id.into(),
            kind: DeliveryKind::Notification,
            target: DeliveryTarget::Notification {
                channel: channel.into(),
                recipient: recipient.into(),
            },
            payload,
            headers: BTreeMap::new(),
            status: DeliveryJobStatus::Pending,
            attempt_count: 0,
            max_attempts: retry_policy.max_attempts,
            available_at: now,
            created_at: now,
            last_error: None,
        }
    }

    pub fn webhook(
        tenant_id: impl Into<String>,
        endpoint_id: impl Into<String>,
        url: impl Into<String>,
        signing_secret: Option<String>,
        payload: serde_json::Value,
        headers: BTreeMap<String, String>,
        retry_policy: RetryPolicy,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::now_v7(),
            tenant_id: tenant_id.into(),
            kind: DeliveryKind::OutboundWebhook,
            target: DeliveryTarget::Webhook {
                endpoint_id: endpoint_id.into(),
                url: url.into(),
                signing_secret,
            },
            payload,
            headers,
            status: DeliveryJobStatus::Pending,
            attempt_count: 0,
            max_attempts: retry_policy.max_attempts,
            available_at: now,
            created_at: now,
            last_error: None,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum DeliveryOutboxError {
    #[error("delivery job not found: {0}")]
    NotFound(Uuid),
    #[error("delivery outbox error: {0}")]
    Store(String),
}

#[async_trait]
pub trait DeliveryOutbox: Send + Sync {
    async fn enqueue(&self, job: DeliveryJob) -> Result<DeliveryJob, DeliveryOutboxError>;
    async fn claim_due(
        &self,
        limit: usize,
        now: DateTime<Utc>,
    ) -> Result<Vec<DeliveryJob>, DeliveryOutboxError>;
    async fn record_attempt(&self, id: Uuid) -> Result<DeliveryJob, DeliveryOutboxError>;
    async fn mark_delivered(&self, id: Uuid) -> Result<DeliveryJob, DeliveryOutboxError>;
    async fn mark_retry(
        &self,
        id: Uuid,
        error: String,
        next_available_at: DateTime<Utc>,
    ) -> Result<DeliveryJob, DeliveryOutboxError>;
    async fn mark_dead_lettered(
        &self,
        id: Uuid,
        error: String,
    ) -> Result<DeliveryJob, DeliveryOutboxError>;
    async fn get(&self, id: Uuid) -> Result<Option<DeliveryJob>, DeliveryOutboxError>;
    async fn dead_letters(&self) -> Result<Vec<DeliveryJob>, DeliveryOutboxError>;
}

#[derive(Debug, Default)]
pub struct InMemoryDeliveryOutbox {
    jobs: tokio::sync::Mutex<HashMap<Uuid, DeliveryJob>>,
}

#[async_trait]
impl DeliveryOutbox for InMemoryDeliveryOutbox {
    async fn enqueue(&self, job: DeliveryJob) -> Result<DeliveryJob, DeliveryOutboxError> {
        let mut jobs = self.jobs.lock().await;
        jobs.insert(job.id, job.clone());
        Ok(job)
    }

    async fn claim_due(
        &self,
        limit: usize,
        now: DateTime<Utc>,
    ) -> Result<Vec<DeliveryJob>, DeliveryOutboxError> {
        let jobs = self.jobs.lock().await;
        let mut due = jobs
            .values()
            .filter(|job| {
                matches!(
                    job.status,
                    DeliveryJobStatus::Pending | DeliveryJobStatus::Failed
                ) && job.available_at <= now
            })
            .cloned()
            .collect::<Vec<_>>();
        due.sort_by_key(|job| job.created_at);
        due.truncate(limit);
        Ok(due)
    }

    async fn record_attempt(&self, id: Uuid) -> Result<DeliveryJob, DeliveryOutboxError> {
        let mut jobs = self.jobs.lock().await;
        let job = jobs.get_mut(&id).ok_or(DeliveryOutboxError::NotFound(id))?;
        job.status = DeliveryJobStatus::InProgress;
        job.attempt_count += 1;
        Ok(job.clone())
    }

    async fn mark_delivered(&self, id: Uuid) -> Result<DeliveryJob, DeliveryOutboxError> {
        let mut jobs = self.jobs.lock().await;
        let job = jobs.get_mut(&id).ok_or(DeliveryOutboxError::NotFound(id))?;
        job.status = DeliveryJobStatus::Delivered;
        job.last_error = None;
        Ok(job.clone())
    }

    async fn mark_retry(
        &self,
        id: Uuid,
        error: String,
        next_available_at: DateTime<Utc>,
    ) -> Result<DeliveryJob, DeliveryOutboxError> {
        let mut jobs = self.jobs.lock().await;
        let job = jobs.get_mut(&id).ok_or(DeliveryOutboxError::NotFound(id))?;
        job.status = DeliveryJobStatus::Failed;
        job.last_error = Some(error);
        job.available_at = next_available_at;
        Ok(job.clone())
    }

    async fn mark_dead_lettered(
        &self,
        id: Uuid,
        error: String,
    ) -> Result<DeliveryJob, DeliveryOutboxError> {
        let mut jobs = self.jobs.lock().await;
        let job = jobs.get_mut(&id).ok_or(DeliveryOutboxError::NotFound(id))?;
        job.status = DeliveryJobStatus::DeadLettered;
        job.last_error = Some(error);
        Ok(job.clone())
    }

    async fn get(&self, id: Uuid) -> Result<Option<DeliveryJob>, DeliveryOutboxError> {
        Ok(self.jobs.lock().await.get(&id).cloned())
    }

    async fn dead_letters(&self) -> Result<Vec<DeliveryJob>, DeliveryOutboxError> {
        Ok(self
            .jobs
            .lock()
            .await
            .values()
            .filter(|job| job.status == DeliveryJobStatus::DeadLettered)
            .cloned()
            .collect())
    }
}
