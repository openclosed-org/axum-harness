//! Runtime security policy contracts for deployable entrypoints.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeProfile {
    Development,
    Test,
    Production,
}

impl RuntimeProfile {
    pub fn from_env() -> Self {
        std::env::var("APP_ENV")
            .or_else(|_| std::env::var("APP_PROFILE"))
            .map(|value| Self::from_label(&value))
            .unwrap_or(Self::Development)
    }

    pub fn from_label(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "production" | "prod" => Self::Production,
            "test" => Self::Test,
            _ => Self::Development,
        }
    }

    pub fn is_production(self) -> bool {
        matches!(self, Self::Production)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{key}: {message}")]
pub struct RuntimeGuardViolation {
    pub key: &'static str,
    pub message: String,
}

impl RuntimeGuardViolation {
    pub fn new(key: &'static str, message: impl Into<String>) -> Self {
        Self {
            key,
            message: message.into(),
        }
    }
}

pub trait RuntimeSecurityPolicy {
    fn validate_runtime_profile(
        &self,
        profile: RuntimeProfile,
    ) -> Result<(), RuntimeGuardViolation>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_profile_parses_production_aliases() {
        assert_eq!(
            RuntimeProfile::from_label("production"),
            RuntimeProfile::Production
        );
        assert_eq!(
            RuntimeProfile::from_label("prod"),
            RuntimeProfile::Production
        );
    }

    #[test]
    fn runtime_profile_defaults_unknown_labels_to_development() {
        assert_eq!(
            RuntimeProfile::from_label("local"),
            RuntimeProfile::Development
        );
    }
}
