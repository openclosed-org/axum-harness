use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BillingWebhookEvent {
    pub provider: String,
    pub provider_event_id: String,
    pub event_type: String,
    pub payload_hash: String,
    pub received_at: DateTime<Utc>,
}

impl BillingWebhookEvent {
    pub fn new(
        provider: impl Into<String>,
        provider_event_id: impl Into<String>,
        event_type: impl Into<String>,
        payload_hash: impl Into<String>,
    ) -> Self {
        Self {
            provider: provider.into(),
            provider_event_id: provider_event_id.into(),
            event_type: event_type.into(),
            payload_hash: payload_hash.into(),
            received_at: Utc::now(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BillingEventStatus {
    Received,
    Processed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BillingWebhookRecord {
    pub event: BillingWebhookEvent,
    pub status: BillingEventStatus,
    pub attempt_count: u32,
    pub last_error: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum BillingWebhookLedgerError {
    #[error("billing webhook event not found: {0}")]
    NotFound(String),
    #[error("billing webhook ledger error: {0}")]
    Ledger(String),
}

#[async_trait]
pub trait BillingWebhookLedger: Send + Sync {
    async fn record_received(
        &self,
        event: BillingWebhookEvent,
    ) -> Result<BillingWebhookRecord, BillingWebhookLedgerError>;

    async fn mark_processed(
        &self,
        provider: &str,
        provider_event_id: &str,
    ) -> Result<(), BillingWebhookLedgerError>;

    async fn mark_failed(
        &self,
        provider: &str,
        provider_event_id: &str,
        error: String,
    ) -> Result<(), BillingWebhookLedgerError>;
}

#[derive(Debug, Default)]
pub struct InMemoryBillingWebhookLedger {
    records: tokio::sync::Mutex<HashMap<String, BillingWebhookRecord>>,
}

impl InMemoryBillingWebhookLedger {
    pub async fn record(
        &self,
        provider: &str,
        provider_event_id: &str,
    ) -> Option<BillingWebhookRecord> {
        self.records
            .lock()
            .await
            .get(&record_key(provider, provider_event_id))
            .cloned()
    }
}

#[async_trait]
impl BillingWebhookLedger for InMemoryBillingWebhookLedger {
    async fn record_received(
        &self,
        event: BillingWebhookEvent,
    ) -> Result<BillingWebhookRecord, BillingWebhookLedgerError> {
        let mut records = self.records.lock().await;
        let key = record_key(&event.provider, &event.provider_event_id);
        if let Some(existing) = records.get(&key) {
            return Ok(existing.clone());
        }

        let record = BillingWebhookRecord {
            event,
            status: BillingEventStatus::Received,
            attempt_count: 0,
            last_error: None,
        };
        records.insert(key, record.clone());
        Ok(record)
    }

    async fn mark_processed(
        &self,
        provider: &str,
        provider_event_id: &str,
    ) -> Result<(), BillingWebhookLedgerError> {
        let mut records = self.records.lock().await;
        let key = record_key(provider, provider_event_id);
        let record = records
            .get_mut(&key)
            .ok_or_else(|| BillingWebhookLedgerError::NotFound(key.clone()))?;
        record.status = BillingEventStatus::Processed;
        record.attempt_count += 1;
        record.last_error = None;
        Ok(())
    }

    async fn mark_failed(
        &self,
        provider: &str,
        provider_event_id: &str,
        error: String,
    ) -> Result<(), BillingWebhookLedgerError> {
        let mut records = self.records.lock().await;
        let key = record_key(provider, provider_event_id);
        let record = records
            .get_mut(&key)
            .ok_or_else(|| BillingWebhookLedgerError::NotFound(key.clone()))?;
        record.status = BillingEventStatus::Failed;
        record.attempt_count += 1;
        record.last_error = Some(error);
        Ok(())
    }
}

fn record_key(provider: &str, provider_event_id: &str) -> String {
    format!("{provider}:{provider_event_id}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn duplicate_provider_event_returns_existing_record() {
        let ledger = InMemoryBillingWebhookLedger::default();
        let event = BillingWebhookEvent::new("mock", "evt_1", "subscription.created", "hash-a");
        let duplicate = BillingWebhookEvent::new("mock", "evt_1", "subscription.created", "hash-b");

        let first = ledger.record_received(event).await.unwrap();
        let second = ledger.record_received(duplicate).await.unwrap();

        assert_eq!(first.event.payload_hash, "hash-a");
        assert_eq!(second.event.payload_hash, "hash-a");
    }

    #[tokio::test]
    async fn received_event_can_be_processed_or_failed() {
        let ledger = InMemoryBillingWebhookLedger::default();
        ledger
            .record_received(BillingWebhookEvent::new(
                "mock",
                "evt_1",
                "subscription.created",
                "hash-a",
            ))
            .await
            .unwrap();

        ledger
            .mark_failed("mock", "evt_1", "temporary error".to_string())
            .await
            .unwrap();
        let failed = ledger.record("mock", "evt_1").await.unwrap();
        assert_eq!(failed.status, BillingEventStatus::Failed);
        assert_eq!(failed.attempt_count, 1);

        ledger.mark_processed("mock", "evt_1").await.unwrap();
        let processed = ledger.record("mock", "evt_1").await.unwrap();
        assert_eq!(processed.status, BillingEventStatus::Processed);
        assert_eq!(processed.attempt_count, 2);
        assert_eq!(processed.last_error, None);
    }
}
