use crate::BotChannelKind;
use async_trait::async_trait;
use commercial::{CommercialSubject, CommercialTenant};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BotUseCaseRequest {
    pub use_case: String,
    pub tenant: CommercialTenant,
    pub subject: CommercialSubject,
    pub channel: BotChannelKind,
    pub command: String,
    pub args: Vec<String>,
    pub request_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BotUseCaseResponse {
    pub message: String,
}

impl BotUseCaseResponse {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum BotUseCaseError {
    #[error("use case rejected command: {0}")]
    Rejected(String),
}

#[async_trait]
pub trait BotUseCasePort: Send + Sync {
    async fn execute(
        &self,
        request: BotUseCaseRequest,
    ) -> Result<BotUseCaseResponse, BotUseCaseError>;
}
