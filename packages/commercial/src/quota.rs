use crate::{CapabilityKey, CommercialSubject, CommercialTenant};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuotaReservation {
    pub reservation_id: String,
    pub tenant: CommercialTenant,
    pub subject: CommercialSubject,
    pub capability: CapabilityKey,
    pub quantity: u64,
    pub status: QuotaReservationStatus,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuotaReservationStatus {
    Reserved,
    Committed,
    Released,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuotaDecision {
    Reserved(QuotaReservation),
    Denied { reason: String },
}

#[derive(Debug, thiserror::Error)]
pub enum QuotaLedgerError {
    #[error("quota reservation not found: {0}")]
    NotFound(String),
    #[error("invalid quota reservation transition: {0}")]
    InvalidTransition(String),
    #[error("quota ledger error: {0}")]
    Ledger(String),
}

#[async_trait]
pub trait QuotaLedger: Send + Sync {
    async fn reserve(
        &self,
        tenant: &CommercialTenant,
        subject: &CommercialSubject,
        capability: &CapabilityKey,
        quantity: u64,
    ) -> Result<QuotaDecision, QuotaLedgerError>;

    async fn commit(&self, reservation_id: &str) -> Result<(), QuotaLedgerError>;

    async fn release(&self, reservation_id: &str) -> Result<(), QuotaLedgerError>;
}

#[derive(Debug, Default)]
pub struct InMemoryQuotaLedger {
    state: tokio::sync::Mutex<QuotaState>,
}

#[derive(Debug, Default)]
struct QuotaState {
    limits: HashMap<String, u64>,
    used: HashMap<String, u64>,
    reservations: HashMap<String, QuotaReservation>,
}

impl InMemoryQuotaLedger {
    pub async fn set_limit(
        &self,
        tenant: &CommercialTenant,
        capability: &CapabilityKey,
        limit: u64,
    ) {
        self.state
            .lock()
            .await
            .limits
            .insert(limit_key(tenant, capability), limit);
    }

    pub async fn reservation(&self, reservation_id: &str) -> Option<QuotaReservation> {
        self.state
            .lock()
            .await
            .reservations
            .get(reservation_id)
            .cloned()
    }

    pub async fn committed_usage(
        &self,
        tenant: &CommercialTenant,
        capability: &CapabilityKey,
    ) -> u64 {
        self.state
            .lock()
            .await
            .used
            .get(&limit_key(tenant, capability))
            .copied()
            .unwrap_or(0)
    }
}

#[async_trait]
impl QuotaLedger for InMemoryQuotaLedger {
    async fn reserve(
        &self,
        tenant: &CommercialTenant,
        subject: &CommercialSubject,
        capability: &CapabilityKey,
        quantity: u64,
    ) -> Result<QuotaDecision, QuotaLedgerError> {
        let mut state = self.state.lock().await;
        let key = limit_key(tenant, capability);
        if let Some(limit) = state.limits.get(&key).copied() {
            let committed = state.used.get(&key).copied().unwrap_or(0);
            let reserved = state
                .reservations
                .values()
                .filter(|reservation| {
                    reservation.tenant == *tenant
                        && reservation.capability == *capability
                        && reservation.status == QuotaReservationStatus::Reserved
                })
                .map(|reservation| reservation.quantity)
                .sum::<u64>();
            if committed + reserved + quantity > limit {
                return Ok(QuotaDecision::Denied {
                    reason: format!("quota exceeded for '{}'", capability.as_str()),
                });
            }
        }

        let reservation = QuotaReservation {
            reservation_id: uuid::Uuid::now_v7().to_string(),
            tenant: tenant.clone(),
            subject: subject.clone(),
            capability: capability.clone(),
            quantity,
            status: QuotaReservationStatus::Reserved,
            created_at: Utc::now(),
        };
        state
            .reservations
            .insert(reservation.reservation_id.clone(), reservation.clone());
        Ok(QuotaDecision::Reserved(reservation))
    }

    async fn commit(&self, reservation_id: &str) -> Result<(), QuotaLedgerError> {
        let mut state = self.state.lock().await;
        let (key, quantity) = {
            let reservation = state
                .reservations
                .get_mut(reservation_id)
                .ok_or_else(|| QuotaLedgerError::NotFound(reservation_id.to_string()))?;
            if reservation.status != QuotaReservationStatus::Reserved {
                return Err(QuotaLedgerError::InvalidTransition(format!(
                    "cannot commit {:?}",
                    reservation.status
                )));
            }
            reservation.status = QuotaReservationStatus::Committed;
            (
                limit_key(&reservation.tenant, &reservation.capability),
                reservation.quantity,
            )
        };
        *state.used.entry(key).or_insert(0) += quantity;
        Ok(())
    }

    async fn release(&self, reservation_id: &str) -> Result<(), QuotaLedgerError> {
        let mut state = self.state.lock().await;
        let reservation = state
            .reservations
            .get_mut(reservation_id)
            .ok_or_else(|| QuotaLedgerError::NotFound(reservation_id.to_string()))?;
        if reservation.status != QuotaReservationStatus::Reserved {
            return Err(QuotaLedgerError::InvalidTransition(format!(
                "cannot release {:?}",
                reservation.status
            )));
        }
        reservation.status = QuotaReservationStatus::Released;
        Ok(())
    }
}

fn limit_key(tenant: &CommercialTenant, capability: &CapabilityKey) -> String {
    format!("{}:{}", tenant.as_str(), capability.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn reserve_commit_and_release_follow_ledger_transitions() {
        let ledger = InMemoryQuotaLedger::default();
        let tenant = CommercialTenant::new("tenant-a");
        let subject = CommercialSubject::new("user-a");
        let capability = CapabilityKey::new("counter.write");
        ledger.set_limit(&tenant, &capability, 2).await;

        let first = ledger
            .reserve(&tenant, &subject, &capability, 1)
            .await
            .unwrap();
        let first = match first {
            QuotaDecision::Reserved(reservation) => reservation,
            QuotaDecision::Denied { reason } => panic!("unexpected denial: {reason}"),
        };
        ledger.commit(&first.reservation_id).await.unwrap();
        assert_eq!(ledger.committed_usage(&tenant, &capability).await, 1);

        let second = match ledger
            .reserve(&tenant, &subject, &capability, 1)
            .await
            .unwrap()
        {
            QuotaDecision::Reserved(reservation) => reservation,
            QuotaDecision::Denied { reason } => panic!("unexpected denial: {reason}"),
        };
        ledger.release(&second.reservation_id).await.unwrap();
        assert_eq!(ledger.committed_usage(&tenant, &capability).await, 1);
    }

    #[tokio::test]
    async fn reservation_denies_when_limit_would_be_exceeded() {
        let ledger = InMemoryQuotaLedger::default();
        let tenant = CommercialTenant::new("tenant-a");
        let subject = CommercialSubject::new("user-a");
        let capability = CapabilityKey::new("counter.write");
        ledger.set_limit(&tenant, &capability, 1).await;

        assert!(matches!(
            ledger
                .reserve(&tenant, &subject, &capability, 1)
                .await
                .unwrap(),
            QuotaDecision::Reserved(_)
        ));
        assert!(matches!(
            ledger
                .reserve(&tenant, &subject, &capability, 1)
                .await
                .unwrap(),
            QuotaDecision::Denied { .. }
        ));
    }

    #[tokio::test]
    async fn committed_reservation_cannot_be_released() {
        let ledger = InMemoryQuotaLedger::default();
        let tenant = CommercialTenant::new("tenant-a");
        let subject = CommercialSubject::new("user-a");
        let capability = CapabilityKey::new("counter.write");
        let reservation = match ledger
            .reserve(&tenant, &subject, &capability, 1)
            .await
            .unwrap()
        {
            QuotaDecision::Reserved(reservation) => reservation,
            QuotaDecision::Denied { reason } => panic!("unexpected denial: {reason}"),
        };

        ledger.commit(&reservation.reservation_id).await.unwrap();
        let error = ledger
            .release(&reservation.reservation_id)
            .await
            .unwrap_err();
        assert!(matches!(error, QuotaLedgerError::InvalidTransition(_)));
    }
}
