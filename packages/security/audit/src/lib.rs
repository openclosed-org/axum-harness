//! Shared security audit contracts.
//!
//! This crate owns the stable audit event shape, redaction rule, and sink port.
//! Deployable-specific durable storage and request extraction stay in entrypoint
//! crates.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuditOutcome {
    Allowed,
    Denied,
    Succeeded,
    Failed,
}

#[derive(Debug, Clone)]
pub struct AuditEvent {
    pub event_id: String,
    pub occurred_at: DateTime<Utc>,
    pub actor_sub: Option<String>,
    pub tenant_id: Option<String>,
    pub action: String,
    pub resource_type: String,
    pub resource_id: String,
    pub outcome: AuditOutcome,
    pub request_id: Option<String>,
    pub trace_id: Option<String>,
    pub metadata: serde_json::Value,
}

impl AuditEvent {
    pub fn new(
        action: impl Into<String>,
        resource_type: impl Into<String>,
        resource_id: impl Into<String>,
        outcome: AuditOutcome,
    ) -> Self {
        Self {
            event_id: uuid::Uuid::now_v7().to_string(),
            occurred_at: Utc::now(),
            actor_sub: None,
            tenant_id: None,
            action: action.into(),
            resource_type: resource_type.into(),
            resource_id: resource_id.into(),
            outcome,
            request_id: None,
            trace_id: None,
            metadata: serde_json::json!({}),
        }
    }

    pub fn actor(mut self, actor_sub: impl Into<String>) -> Self {
        self.actor_sub = Some(actor_sub.into());
        self
    }

    pub fn tenant(mut self, tenant_id: impl Into<String>) -> Self {
        self.tenant_id = Some(tenant_id.into());
        self
    }

    pub fn request(mut self, request_id: Option<String>, trace_id: Option<String>) -> Self {
        self.request_id = request_id;
        self.trace_id = trace_id;
        self
    }

    pub fn metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = redact_metadata(metadata);
        self
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AuditError {
    #[error("audit sink error: {0}")]
    Sink(String),
}

#[async_trait]
pub trait AuditSink: Send + Sync {
    async fn append(&self, event: AuditEvent) -> Result<(), AuditError>;
}

#[derive(Debug, Default)]
pub struct InMemoryAuditSink {
    events: tokio::sync::Mutex<Vec<AuditEvent>>,
}

impl InMemoryAuditSink {
    pub fn shared() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub async fn events(&self) -> Vec<AuditEvent> {
        self.events.lock().await.clone()
    }
}

#[async_trait]
impl AuditSink for InMemoryAuditSink {
    async fn append(&self, event: AuditEvent) -> Result<(), AuditError> {
        self.events.lock().await.push(event);
        Ok(())
    }
}

pub fn redact_metadata(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => serde_json::Value::Object(
            map.into_iter()
                .map(|(key, value)| {
                    if is_sensitive_key(&key) {
                        (key, serde_json::Value::String("[redacted]".to_string()))
                    } else {
                        (key, redact_metadata(value))
                    }
                })
                .collect(),
        ),
        serde_json::Value::Array(values) => {
            serde_json::Value::Array(values.into_iter().map(redact_metadata).collect())
        }
        other => other,
    }
}

fn is_sensitive_key(key: &str) -> bool {
    matches!(
        key.to_ascii_lowercase().as_str(),
        "authorization" | "token" | "bearer" | "jwt" | "idempotency_key" | "idempotency-key"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_sensitive_metadata_recursively() {
        let metadata = serde_json::json!({
            "authorization": "Bearer raw-token",
            "nested": {
                "idempotency_key": "raw-key",
                "safe": "visible"
            },
            "array": [{ "jwt": "raw-jwt" }]
        });

        let redacted = redact_metadata(metadata);

        assert_eq!(redacted["authorization"], "[redacted]");
        assert_eq!(redacted["nested"]["idempotency_key"], "[redacted]");
        assert_eq!(redacted["nested"]["safe"], "visible");
        assert_eq!(redacted["array"][0]["jwt"], "[redacted]");
    }

    #[tokio::test]
    async fn in_memory_sink_appends_events_in_order() {
        let sink = InMemoryAuditSink::default();

        sink.append(AuditEvent::new(
            "first",
            "resource",
            "1",
            AuditOutcome::Allowed,
        ))
        .await
        .unwrap();
        sink.append(AuditEvent::new(
            "second",
            "resource",
            "2",
            AuditOutcome::Denied,
        ))
        .await
        .unwrap();

        let events = sink.events().await;
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].action, "first");
        assert_eq!(events[1].action, "second");
    }
}
