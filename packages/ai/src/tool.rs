use async_trait::async_trait;
use commercial::{CommercialSubject, CommercialTenant};
use std::collections::HashSet;
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ToolKey(String);

impl ToolKey {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for ToolKey {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolPermissionDecision {
    Allowed,
    Denied { reason: String },
}

#[derive(Debug, thiserror::Error)]
pub enum ToolPermissionError {
    #[error("tool permission error: {0}")]
    Policy(String),
}

#[async_trait]
pub trait ToolPermissionPolicy: Send + Sync {
    async fn check_tool(
        &self,
        tenant: &CommercialTenant,
        subject: &CommercialSubject,
        tool: &ToolKey,
    ) -> Result<ToolPermissionDecision, ToolPermissionError>;
}

#[derive(Debug, Clone, Default)]
pub struct StaticToolPermissionPolicy {
    allowed_tools: Arc<HashSet<String>>,
}

impl StaticToolPermissionPolicy {
    pub fn allow_tools(tools: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            allowed_tools: Arc::new(tools.into_iter().map(Into::into).collect()),
        }
    }
}

#[async_trait]
impl ToolPermissionPolicy for StaticToolPermissionPolicy {
    async fn check_tool(
        &self,
        _tenant: &CommercialTenant,
        _subject: &CommercialSubject,
        tool: &ToolKey,
    ) -> Result<ToolPermissionDecision, ToolPermissionError> {
        if self.allowed_tools.contains(tool.as_str()) {
            Ok(ToolPermissionDecision::Allowed)
        } else {
            Ok(ToolPermissionDecision::Denied {
                reason: format!("tool '{}' is not allowed", tool.as_str()),
            })
        }
    }
}
