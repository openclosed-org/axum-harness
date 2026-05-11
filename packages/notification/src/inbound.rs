use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InboundWebhookEvent {
    pub provider: String,
    pub provider_event_id: String,
    pub payload_hash: String,
    pub received_at: DateTime<Utc>,
}

impl InboundWebhookEvent {
    pub fn new(
        provider: impl Into<String>,
        provider_event_id: impl Into<String>,
        payload_hash: impl Into<String>,
    ) -> Self {
        Self {
            provider: provider.into(),
            provider_event_id: provider_event_id.into(),
            payload_hash: payload_hash.into(),
            received_at: Utc::now(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum InboundWebhookLedgerError {
    #[error("inbound webhook ledger error: {0}")]
    Ledger(String),
}

#[async_trait]
pub trait InboundWebhookLedger: Send + Sync {
    async fn record_received(
        &self,
        event: InboundWebhookEvent,
    ) -> Result<bool, InboundWebhookLedgerError>;
}

#[derive(Debug, Default)]
pub struct InMemoryInboundWebhookLedger {
    events: tokio::sync::Mutex<HashMap<String, InboundWebhookEvent>>,
}

#[async_trait]
impl InboundWebhookLedger for InMemoryInboundWebhookLedger {
    async fn record_received(
        &self,
        event: InboundWebhookEvent,
    ) -> Result<bool, InboundWebhookLedgerError> {
        let key = format!("{}:{}", event.provider, event.provider_event_id);
        let mut events = self.events.lock().await;
        if events.contains_key(&key) {
            return Ok(false);
        }
        events.insert(key, event);
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn inbound_webhook_ledger_reports_duplicates() {
        let ledger = InMemoryInboundWebhookLedger::default();
        let first = ledger
            .record_received(InboundWebhookEvent::new("provider", "evt_1", "hash-a"))
            .await
            .unwrap();
        let duplicate = ledger
            .record_received(InboundWebhookEvent::new("provider", "evt_1", "hash-b"))
            .await
            .unwrap();

        assert!(first);
        assert!(!duplicate);
    }
}
