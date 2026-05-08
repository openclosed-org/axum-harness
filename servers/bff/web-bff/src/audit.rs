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

#[derive(Debug, Default)]
pub struct InMemoryAuditSink {
    events: tokio::sync::Mutex<Vec<AuditEvent>>,
}

impl InMemoryAuditSink {
    pub fn shared() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub async fn append(&self, event: AuditEvent) {
        self.events.lock().await.push(event);
    }

    pub async fn events(&self) -> Vec<AuditEvent> {
        self.events.lock().await.clone()
    }
}

fn redact_metadata(value: serde_json::Value) -> serde_json::Value {
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
