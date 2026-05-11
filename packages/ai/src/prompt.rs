use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptVersion {
    pub prompt_key: String,
    pub version: String,
    pub template: String,
}

impl PromptVersion {
    pub fn new(
        prompt_key: impl Into<String>,
        version: impl Into<String>,
        template: impl Into<String>,
    ) -> Self {
        Self {
            prompt_key: prompt_key.into(),
            version: version.into(),
            template: template.into(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PromptRegistryError {
    #[error("prompt not found: {0}")]
    NotFound(String),
    #[error("prompt registry error: {0}")]
    Registry(String),
}

#[async_trait]
pub trait PromptRegistry: Send + Sync {
    async fn select_prompt(
        &self,
        prompt_key: &str,
        version: Option<&str>,
    ) -> Result<PromptVersion, PromptRegistryError>;
}

#[derive(Debug, Default)]
pub struct InMemoryPromptRegistry {
    prompts: tokio::sync::Mutex<HashMap<String, Vec<PromptVersion>>>,
}

impl InMemoryPromptRegistry {
    pub fn shared() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub async fn register(&self, prompt: PromptVersion) {
        self.prompts
            .lock()
            .await
            .entry(prompt.prompt_key.clone())
            .or_default()
            .push(prompt);
    }
}

#[async_trait]
impl PromptRegistry for InMemoryPromptRegistry {
    async fn select_prompt(
        &self,
        prompt_key: &str,
        version: Option<&str>,
    ) -> Result<PromptVersion, PromptRegistryError> {
        let prompts = self.prompts.lock().await;
        let versions = prompts
            .get(prompt_key)
            .ok_or_else(|| PromptRegistryError::NotFound(prompt_key.to_string()))?;

        if let Some(version) = version {
            return versions
                .iter()
                .find(|prompt| prompt.version == version)
                .cloned()
                .ok_or_else(|| PromptRegistryError::NotFound(format!("{prompt_key}@{version}")));
        }

        versions
            .last()
            .cloned()
            .ok_or_else(|| PromptRegistryError::NotFound(prompt_key.to_string()))
    }
}
