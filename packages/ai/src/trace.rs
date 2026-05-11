use async_trait::async_trait;
use chrono::{DateTime, Utc};
use commercial::{CapabilityKey, CommercialSubject, CommercialTenant};
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AiTraceOutcome {
    Succeeded,
    Denied,
    Failed,
}

#[derive(Debug, Clone)]
pub struct AiTraceEvent {
    pub trace_id: String,
    pub occurred_at: DateTime<Utc>,
    pub invocation_id: String,
    pub tenant: CommercialTenant,
    pub subject: CommercialSubject,
    pub capability: CapabilityKey,
    pub prompt_key: String,
    pub prompt_version: Option<String>,
    pub provider_request_id: Option<String>,
    pub model: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub outcome: AiTraceOutcome,
    pub error: Option<String>,
    pub metadata: serde_json::Value,
}

#[derive(Debug, thiserror::Error)]
pub enum AiTraceStoreError {
    #[error("ai trace store error: {0}")]
    Store(String),
}

#[async_trait]
pub trait AiTraceStore: Send + Sync {
    async fn append(&self, event: AiTraceEvent) -> Result<(), AiTraceStoreError>;
}

#[derive(Debug, Default)]
pub struct InMemoryAiTraceStore {
    events: tokio::sync::Mutex<Vec<AiTraceEvent>>,
}

impl InMemoryAiTraceStore {
    pub fn shared() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub async fn events(&self) -> Vec<AiTraceEvent> {
        self.events.lock().await.clone()
    }
}

#[async_trait]
impl AiTraceStore for InMemoryAiTraceStore {
    async fn append(&self, event: AiTraceEvent) -> Result<(), AiTraceStoreError> {
        self.events.lock().await.push(event);
        Ok(())
    }
}
