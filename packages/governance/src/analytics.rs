use async_trait::async_trait;
use chrono::{DateTime, Utc};
use security_audit::redact_metadata;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProductEventMode {
    Disabled,
    LocalMock,
    LocalReal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductEvent {
    pub event_id: String,
    pub occurred_at: DateTime<Utc>,
    pub actor_sub: Option<String>,
    pub tenant_id: Option<String>,
    pub event_name: String,
    pub resource_type: String,
    pub resource_id: String,
    pub request_id: Option<String>,
    pub trace_id: Option<String>,
    pub properties: serde_json::Value,
}

impl ProductEvent {
    pub fn new(
        event_name: impl Into<String>,
        resource_type: impl Into<String>,
        resource_id: impl Into<String>,
    ) -> Self {
        Self {
            event_id: uuid::Uuid::now_v7().to_string(),
            occurred_at: Utc::now(),
            actor_sub: None,
            tenant_id: None,
            event_name: event_name.into(),
            resource_type: resource_type.into(),
            resource_id: resource_id.into(),
            request_id: None,
            trace_id: None,
            properties: serde_json::json!({}),
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

    pub fn properties(mut self, properties: serde_json::Value) -> Self {
        self.properties = redact_metadata(properties);
        self
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ProductEventError {
    #[error("product event recorder error: {0}")]
    Recorder(String),
}

#[async_trait]
pub trait ProductEventRecorder: Send + Sync {
    async fn record(&self, event: ProductEvent) -> Result<(), ProductEventError>;
}

#[derive(Debug, Default)]
pub struct InMemoryProductEventRecorder {
    events: tokio::sync::Mutex<Vec<ProductEvent>>,
}

impl InMemoryProductEventRecorder {
    pub fn shared() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub async fn events(&self) -> Vec<ProductEvent> {
        self.events.lock().await.clone()
    }
}

#[async_trait]
impl ProductEventRecorder for InMemoryProductEventRecorder {
    async fn record(&self, event: ProductEvent) -> Result<(), ProductEventError> {
        self.events.lock().await.push(event);
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProductEventOutcome {
    SkippedDisabled,
    Recorded,
}

pub struct ProductEventService {
    mode: ProductEventMode,
    recorder: Arc<dyn ProductEventRecorder>,
}

impl ProductEventService {
    pub fn new(mode: ProductEventMode, recorder: Arc<dyn ProductEventRecorder>) -> Self {
        Self { mode, recorder }
    }

    pub async fn record(
        &self,
        event: ProductEvent,
    ) -> Result<ProductEventOutcome, ProductEventError> {
        if self.mode == ProductEventMode::Disabled {
            return Ok(ProductEventOutcome::SkippedDisabled);
        }

        self.recorder.record(event).await?;
        Ok(ProductEventOutcome::Recorded)
    }
}
