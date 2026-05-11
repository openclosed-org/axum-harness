//! Commercial capability ports and local Phase 1 implementations.
//!
//! This crate owns framework-neutral commercial seams. Real billing providers
//! belong behind future adapters; business and server code should depend on
//! these local ports and projections instead of provider SDKs.

pub mod billing;
pub mod entitlement;
pub mod quota;
pub mod usage;

pub use billing::{
    BillingEventStatus, BillingWebhookEvent, BillingWebhookLedger, BillingWebhookLedgerError,
    BillingWebhookRecord, InMemoryBillingWebhookLedger,
};
pub use entitlement::{
    EntitlementDecision, EntitlementError, EntitlementResolver, StaticEntitlementResolver,
};
pub use quota::{
    InMemoryQuotaLedger, QuotaDecision, QuotaLedger, QuotaLedgerError, QuotaReservation,
    QuotaReservationStatus,
};
pub use usage::{InMemoryUsageMeter, UsageEvent, UsageMeter, UsageMeterError};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CapabilityKey(String);

impl CapabilityKey {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for CapabilityKey {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CommercialSubject(String);

impl CommercialSubject {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for CommercialSubject {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CommercialTenant(String);

impl CommercialTenant {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for CommercialTenant {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}
