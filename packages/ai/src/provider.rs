use crate::ToolKey;
use async_trait::async_trait;
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LlmProviderRequest {
    pub invocation_id: String,
    pub model: String,
    pub system_prompt: String,
    pub user_input: String,
    pub tools: Vec<ToolKey>,
    pub max_tokens: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LlmProviderResponse {
    pub provider_request_id: String,
    pub output_text: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub finish_reason: String,
}

impl LlmProviderResponse {
    pub fn total_tokens(&self) -> u64 {
        self.input_tokens + self.output_tokens
    }
}

#[derive(Debug, thiserror::Error)]
pub enum LlmProviderError {
    #[error("llm provider error: {0}")]
    Provider(String),
}

#[async_trait]
pub trait LlmProviderPort: Send + Sync {
    async fn complete(
        &self,
        request: LlmProviderRequest,
    ) -> Result<LlmProviderResponse, LlmProviderError>;
}

#[derive(Debug)]
pub struct MockLlmProvider {
    response: LlmProviderResponse,
    requests: tokio::sync::Mutex<Vec<LlmProviderRequest>>,
}

impl MockLlmProvider {
    pub fn shared(response: LlmProviderResponse) -> Arc<Self> {
        Arc::new(Self {
            response,
            requests: tokio::sync::Mutex::new(Vec::new()),
        })
    }

    pub async fn requests(&self) -> Vec<LlmProviderRequest> {
        self.requests.lock().await.clone()
    }
}

#[async_trait]
impl LlmProviderPort for MockLlmProvider {
    async fn complete(
        &self,
        request: LlmProviderRequest,
    ) -> Result<LlmProviderResponse, LlmProviderError> {
        self.requests.lock().await.push(request);
        Ok(self.response.clone())
    }
}
