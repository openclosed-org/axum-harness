use crate::{CommercialSubject, CommercialTenant};
use async_trait::async_trait;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageEvent {
    pub event_id: String,
    pub occurred_at: DateTime<Utc>,
    pub tenant: CommercialTenant,
    pub subject: CommercialSubject,
    pub meter_name: String,
    pub quantity: u64,
    pub unit: String,
    pub resource_type: String,
    pub resource_id: String,
    pub idempotency_key: Option<String>,
    pub metadata: serde_json::Value,
}

impl UsageEvent {
    pub fn new(
        tenant: CommercialTenant,
        subject: CommercialSubject,
        meter_name: impl Into<String>,
        quantity: u64,
        unit: impl Into<String>,
        resource_type: impl Into<String>,
        resource_id: impl Into<String>,
    ) -> Self {
        Self {
            event_id: uuid::Uuid::now_v7().to_string(),
            occurred_at: Utc::now(),
            tenant,
            subject,
            meter_name: meter_name.into(),
            quantity,
            unit: unit.into(),
            resource_type: resource_type.into(),
            resource_id: resource_id.into(),
            idempotency_key: None,
            metadata: serde_json::json!({}),
        }
    }

    pub fn idempotency_key(mut self, key: impl Into<String>) -> Self {
        self.idempotency_key = Some(key.into());
        self
    }

    pub fn metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = metadata;
        self
    }
}

#[derive(Debug, thiserror::Error)]
pub enum UsageMeterError {
    #[error("usage meter error: {0}")]
    Meter(String),
}

#[async_trait]
pub trait UsageMeter: Send + Sync {
    async fn record(&self, event: UsageEvent) -> Result<(), UsageMeterError>;
}

#[derive(Debug, Default)]
pub struct InMemoryUsageMeter {
    events: tokio::sync::Mutex<Vec<UsageEvent>>,
}

impl InMemoryUsageMeter {
    pub async fn events(&self) -> Vec<UsageEvent> {
        self.events.lock().await.clone()
    }
}

#[async_trait]
impl UsageMeter for InMemoryUsageMeter {
    async fn record(&self, event: UsageEvent) -> Result<(), UsageMeterError> {
        self.events.lock().await.push(event);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn usage_events_are_append_only_and_ordered() {
        let meter = InMemoryUsageMeter::default();

        meter
            .record(UsageEvent::new(
                CommercialTenant::new("tenant-a"),
                CommercialSubject::new("user-a"),
                "counter.write",
                1,
                "operation",
                "counter",
                "tenant-a",
            ))
            .await
            .unwrap();
        meter
            .record(UsageEvent::new(
                CommercialTenant::new("tenant-a"),
                CommercialSubject::new("user-a"),
                "counter.write",
                2,
                "operation",
                "counter",
                "tenant-a",
            ))
            .await
            .unwrap();

        let events = meter.events().await;
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].quantity, 1);
        assert_eq!(events[1].quantity, 2);
    }
}
