use crate::{CapabilityKey, CommercialSubject, CommercialTenant};
use async_trait::async_trait;
use std::collections::HashSet;
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntitlementDecision {
    Allowed,
    Denied { reason: String },
}

impl EntitlementDecision {
    pub fn is_allowed(&self) -> bool {
        matches!(self, Self::Allowed)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum EntitlementError {
    #[error("entitlement denied: {0}")]
    Denied(String),
    #[error("entitlement resolver error: {0}")]
    Resolver(String),
}

#[async_trait]
pub trait EntitlementResolver: Send + Sync {
    async fn check(
        &self,
        tenant: &CommercialTenant,
        subject: &CommercialSubject,
        capability: &CapabilityKey,
    ) -> Result<EntitlementDecision, EntitlementError>;
}

#[derive(Debug, Clone)]
pub struct StaticEntitlementResolver {
    mode: StaticEntitlementMode,
    allowed: Arc<HashSet<String>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StaticEntitlementMode {
    Disabled,
    AllowListed,
}

impl StaticEntitlementResolver {
    pub fn disabled() -> Self {
        Self {
            mode: StaticEntitlementMode::Disabled,
            allowed: Arc::new(HashSet::new()),
        }
    }

    pub fn allow_list(capabilities: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            mode: StaticEntitlementMode::AllowListed,
            allowed: Arc::new(capabilities.into_iter().map(Into::into).collect()),
        }
    }
}

#[async_trait]
impl EntitlementResolver for StaticEntitlementResolver {
    async fn check(
        &self,
        _tenant: &CommercialTenant,
        _subject: &CommercialSubject,
        capability: &CapabilityKey,
    ) -> Result<EntitlementDecision, EntitlementError> {
        if self.mode == StaticEntitlementMode::Disabled {
            return Ok(EntitlementDecision::Allowed);
        }

        if self.allowed.contains(capability.as_str()) {
            Ok(EntitlementDecision::Allowed)
        } else {
            Ok(EntitlementDecision::Denied {
                reason: format!("capability '{}' is not entitled", capability.as_str()),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn disabled_resolver_allows_every_capability() {
        let resolver = StaticEntitlementResolver::disabled();

        let decision = resolver
            .check(
                &CommercialTenant::new("tenant-a"),
                &CommercialSubject::new("user-a"),
                &CapabilityKey::new("counter.write"),
            )
            .await
            .unwrap();

        assert_eq!(decision, EntitlementDecision::Allowed);
    }

    #[tokio::test]
    async fn allow_list_permits_known_capability() {
        let resolver = StaticEntitlementResolver::allow_list(["counter.write"]);

        let decision = resolver
            .check(
                &CommercialTenant::new("tenant-a"),
                &CommercialSubject::new("user-a"),
                &CapabilityKey::new("counter.write"),
            )
            .await
            .unwrap();

        assert!(decision.is_allowed());
    }

    #[tokio::test]
    async fn allow_list_denies_unknown_capability() {
        let resolver = StaticEntitlementResolver::allow_list(["counter.read"]);

        let decision = resolver
            .check(
                &CommercialTenant::new("tenant-a"),
                &CommercialSubject::new("user-a"),
                &CapabilityKey::new("counter.write"),
            )
            .await
            .unwrap();

        assert!(matches!(decision, EntitlementDecision::Denied { .. }));
    }
}
