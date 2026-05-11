use async_trait::async_trait;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeatureFlagMode {
    Static,
    Config,
    Db,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FeatureFlagKey(String);

impl FeatureFlagKey {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for FeatureFlagKey {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FeatureFlagSubject {
    pub actor_sub: Option<String>,
    pub tenant_id: Option<String>,
    pub attributes: HashMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeatureFlagRule {
    pub enabled: bool,
    pub variant: Option<String>,
}

impl FeatureFlagRule {
    pub fn enabled() -> Self {
        Self {
            enabled: true,
            variant: None,
        }
    }

    pub fn disabled() -> Self {
        Self {
            enabled: false,
            variant: None,
        }
    }

    pub fn variant(mut self, variant: impl Into<String>) -> Self {
        self.variant = Some(variant.into());
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeatureFlagDecision {
    pub enabled: bool,
    pub variant: Option<String>,
    pub reason: String,
}

#[derive(Debug, thiserror::Error)]
pub enum FeatureFlagError {
    #[error("feature flag source is read-only in {0:?} mode")]
    ReadOnly(FeatureFlagMode),
}

#[async_trait]
pub trait FeatureFlagProvider: Send + Sync {
    async fn evaluate(
        &self,
        key: &FeatureFlagKey,
        subject: &FeatureFlagSubject,
        default_enabled: bool,
    ) -> Result<FeatureFlagDecision, FeatureFlagError>;
}

#[derive(Debug)]
pub struct RuntimeFeatureFlagProvider {
    mode: FeatureFlagMode,
    rules: tokio::sync::Mutex<HashMap<String, FeatureFlagRule>>,
}

impl RuntimeFeatureFlagProvider {
    pub fn static_flags(
        rules: impl IntoIterator<Item = (impl Into<String>, FeatureFlagRule)>,
    ) -> Self {
        Self::new(FeatureFlagMode::Static, rules)
    }

    pub fn config_flags(
        rules: impl IntoIterator<Item = (impl Into<String>, FeatureFlagRule)>,
    ) -> Self {
        Self::new(FeatureFlagMode::Config, rules)
    }

    pub fn db_backed(
        rules: impl IntoIterator<Item = (impl Into<String>, FeatureFlagRule)>,
    ) -> Self {
        Self::new(FeatureFlagMode::Db, rules)
    }

    fn new(
        mode: FeatureFlagMode,
        rules: impl IntoIterator<Item = (impl Into<String>, FeatureFlagRule)>,
    ) -> Self {
        Self {
            mode,
            rules: tokio::sync::Mutex::new(
                rules
                    .into_iter()
                    .map(|(key, rule)| (key.into(), rule))
                    .collect(),
            ),
        }
    }

    pub async fn set_flag(
        &self,
        key: impl Into<String>,
        rule: FeatureFlagRule,
    ) -> Result<(), FeatureFlagError> {
        if self.mode != FeatureFlagMode::Db {
            return Err(FeatureFlagError::ReadOnly(self.mode));
        }

        self.rules.lock().await.insert(key.into(), rule);
        Ok(())
    }
}

#[async_trait]
impl FeatureFlagProvider for RuntimeFeatureFlagProvider {
    async fn evaluate(
        &self,
        key: &FeatureFlagKey,
        _subject: &FeatureFlagSubject,
        default_enabled: bool,
    ) -> Result<FeatureFlagDecision, FeatureFlagError> {
        let rules = self.rules.lock().await;
        if let Some(rule) = rules.get(key.as_str()) {
            return Ok(FeatureFlagDecision {
                enabled: rule.enabled,
                variant: rule.variant.clone(),
                reason: format!("runtime {:?} rule", self.mode),
            });
        }

        Ok(FeatureFlagDecision {
            enabled: default_enabled,
            variant: None,
            reason: "default".to_string(),
        })
    }
}
