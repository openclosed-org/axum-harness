//! BFF-local commercial composition for Phase 1.
//!
//! Real provider adapters stay out of this module. The BFF only composes local
//! ports, applies guards around use cases, and records usage after success.

use crate::config::{Config, ImplementedCommercialMode};
use crate::error::BffError;
use crate::state::DatabaseBackend;
use ::commercial::{
    CapabilityKey, CommercialSubject, CommercialTenant, EntitlementDecision, EntitlementResolver,
    InMemoryQuotaLedger, InMemoryUsageMeter, QuotaDecision, QuotaLedger, StaticEntitlementResolver,
    UsageEvent, UsageMeter,
};
use async_trait::async_trait;
use data::ports::lib_sql::LibSqlPort;
use serde::Deserialize;
use std::sync::Arc;
use storage_turso::TursoBackend;

pub const COMMERCIAL_LEDGER_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS commercial_quota_limits (
    tenant_id TEXT NOT NULL,
    capability TEXT NOT NULL,
    limit_value INTEGER NOT NULL,
    PRIMARY KEY (tenant_id, capability)
);

CREATE TABLE IF NOT EXISTS commercial_quota_reservations (
    reservation_id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    subject_id TEXT NOT NULL,
    capability TEXT NOT NULL,
    quantity INTEGER NOT NULL,
    status TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_commercial_quota_reservations_limit
    ON commercial_quota_reservations(tenant_id, capability, status);

CREATE TABLE IF NOT EXISTS commercial_usage_events (
    event_id TEXT PRIMARY KEY,
    occurred_at TEXT NOT NULL,
    tenant_id TEXT NOT NULL,
    subject_id TEXT NOT NULL,
    meter_name TEXT NOT NULL,
    quantity INTEGER NOT NULL,
    unit TEXT NOT NULL,
    resource_type TEXT NOT NULL,
    resource_id TEXT NOT NULL,
    idempotency_key TEXT,
    metadata TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_commercial_usage_events_tenant_occurred
    ON commercial_usage_events(tenant_id, occurred_at);

CREATE TABLE IF NOT EXISTS commercial_entitlements (
    tenant_id TEXT NOT NULL,
    subject_id TEXT NOT NULL,
    capability TEXT NOT NULL,
    source_provider TEXT NOT NULL,
    source_customer_id TEXT,
    source_subscription_id TEXT,
    status TEXT NOT NULL,
    granted_at TEXT NOT NULL,
    revoked_at TEXT,
    PRIMARY KEY (tenant_id, subject_id, capability, source_provider)
);

CREATE INDEX IF NOT EXISTS idx_commercial_entitlements_lookup
    ON commercial_entitlements(tenant_id, subject_id, capability, status);
"#;

#[derive(Clone)]
pub struct CommercialStack {
    enabled: bool,
    counter_paid_capability: CapabilityKey,
    entitlement: Arc<dyn EntitlementResolver>,
    quota: Arc<dyn QuotaLedger>,
    usage: Arc<dyn UsageMeter>,
    inspection: CommercialInspection,
}

#[derive(Clone)]
enum CommercialInspection {
    None,
    InMemory {
        quota: Arc<InMemoryQuotaLedger>,
        usage: Arc<InMemoryUsageMeter>,
    },
    LibSql {
        quota: Arc<LibSqlQuotaLedger>,
        usage: Arc<LibSqlUsageMeter>,
    },
}

pub struct CommercialReservation {
    reservation_id: Option<String>,
}

impl CommercialStack {
    pub fn from_config(config: &Config, db: Option<DatabaseBackend>) -> anyhow::Result<Self> {
        let commercial_mode = config
            .implemented_commercial_mode()
            .map_err(anyhow::Error::from)?;
        if commercial_mode == ImplementedCommercialMode::Disabled {
            return Ok(Self::disabled(&config.counter_paid_capability));
        }

        if commercial_mode == ImplementedCommercialMode::LocalMock {
            return Ok(Self::local_mock(
                &config.counter_paid_capability,
                config.commercial_mock_allowed_capabilities.iter().cloned(),
            ));
        }

        let backend = match db {
            Some(DatabaseBackend::Embedded(db)) => TursoBackend::Embedded(db),
            Some(DatabaseBackend::Remote(db)) => TursoBackend::Remote(db),
            None => anyhow::bail!(
                "APP_COMMERCIAL_MODE={} requires APP_DATABASE_URL or Turso configuration",
                config.commercial_mode
            ),
        };

        Self::local_real(
            &config.counter_paid_capability,
            Vec::<String>::new(),
            backend,
        )
    }

    pub fn disabled(counter_paid_capability: &str) -> Self {
        Self {
            enabled: false,
            counter_paid_capability: CapabilityKey::new(counter_paid_capability),
            entitlement: Arc::new(StaticEntitlementResolver::disabled()),
            quota: Arc::new(InMemoryQuotaLedger::default()),
            usage: Arc::new(InMemoryUsageMeter::default()),
            inspection: CommercialInspection::None,
        }
    }

    pub fn local_mock(
        counter_paid_capability: &str,
        allowed_capabilities: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        let quota = Arc::new(InMemoryQuotaLedger::default());
        let usage = Arc::new(InMemoryUsageMeter::default());

        Self {
            enabled: true,
            counter_paid_capability: CapabilityKey::new(counter_paid_capability),
            entitlement: Arc::new(StaticEntitlementResolver::allow_list(allowed_capabilities)),
            quota: quota.clone(),
            usage: usage.clone(),
            inspection: CommercialInspection::InMemory { quota, usage },
        }
    }

    pub fn local_real(
        counter_paid_capability: &str,
        allowed_capabilities: impl IntoIterator<Item = impl Into<String>>,
        backend: TursoBackend,
    ) -> anyhow::Result<Self> {
        let quota = Arc::new(LibSqlQuotaLedger::new(backend.clone()));
        let usage = Arc::new(LibSqlUsageMeter::new(backend.clone()));
        let entitlement = Arc::new(LibSqlEntitlementResolver::new(
            backend,
            allowed_capabilities
                .into_iter()
                .map(Into::into)
                .collect::<Vec<_>>(),
        ));

        Ok(Self {
            enabled: true,
            counter_paid_capability: CapabilityKey::new(counter_paid_capability),
            entitlement,
            quota: quota.clone(),
            usage: usage.clone(),
            inspection: CommercialInspection::LibSql { quota, usage },
        })
    }

    pub async fn reserve_counter_write(
        &self,
        tenant_id: &str,
        subject: &str,
    ) -> Result<CommercialReservation, BffError> {
        if !self.enabled {
            return Ok(CommercialReservation {
                reservation_id: None,
            });
        }

        let tenant = CommercialTenant::new(tenant_id);
        let subject = CommercialSubject::new(subject);
        match self
            .entitlement
            .check(&tenant, &subject, &self.counter_paid_capability)
            .await
            .map_err(|error| BffError::Dependency(error.to_string()))?
        {
            EntitlementDecision::Allowed => {}
            EntitlementDecision::Denied { reason } => return Err(BffError::Forbidden(reason)),
        }

        match self
            .quota
            .reserve(&tenant, &subject, &self.counter_paid_capability, 1)
            .await
            .map_err(|error| BffError::Dependency(error.to_string()))?
        {
            QuotaDecision::Reserved(reservation) => Ok(CommercialReservation {
                reservation_id: Some(reservation.reservation_id),
            }),
            QuotaDecision::Denied { reason } => Err(BffError::Forbidden(reason)),
        }
    }

    pub async fn commit_counter_write(
        &self,
        tenant_id: &str,
        subject: &str,
        reservation: CommercialReservation,
    ) -> Result<(), BffError> {
        if !self.enabled {
            return Ok(());
        }

        if let Some(reservation_id) = reservation.reservation_id {
            self.quota
                .commit(&reservation_id)
                .await
                .map_err(|error| BffError::Dependency(error.to_string()))?;
        }

        self.usage
            .record(UsageEvent::new(
                CommercialTenant::new(tenant_id),
                CommercialSubject::new(subject),
                self.counter_paid_capability.as_str(),
                1,
                "operation",
                "counter",
                tenant_id,
            ))
            .await
            .map_err(|error| BffError::Dependency(error.to_string()))?;

        Ok(())
    }

    pub async fn release_counter_write(&self, reservation: CommercialReservation) {
        if !self.enabled {
            return;
        }

        if let Some(reservation_id) = reservation.reservation_id
            && let Err(error) = self.quota.release(&reservation_id).await
        {
            tracing::warn!(error = %error, "failed to release commercial quota reservation");
        }
    }
}

#[derive(Clone)]
pub struct LibSqlEntitlementProjection {
    port: TursoBackend,
}

impl LibSqlEntitlementProjection {
    pub fn new(port: TursoBackend) -> Self {
        Self { port }
    }

    pub async fn grant(
        &self,
        tenant_id: &str,
        subject_id: &str,
        capability: &str,
        source_provider: &str,
        source_customer_id: Option<&str>,
        source_subscription_id: Option<&str>,
    ) -> anyhow::Result<()> {
        self.port
            .execute(
                "INSERT INTO commercial_entitlements (tenant_id, subject_id, capability, source_provider, source_customer_id, source_subscription_id, status, granted_at, revoked_at) VALUES (?, ?, ?, ?, ?, ?, 'active', ?, NULL) ON CONFLICT(tenant_id, subject_id, capability, source_provider) DO UPDATE SET source_customer_id = excluded.source_customer_id, source_subscription_id = excluded.source_subscription_id, status = 'active', granted_at = excluded.granted_at, revoked_at = NULL",
                vec![
                    tenant_id.to_string(),
                    subject_id.to_string(),
                    capability.to_string(),
                    source_provider.to_string(),
                    source_customer_id.unwrap_or_default().to_string(),
                    source_subscription_id.unwrap_or_default().to_string(),
                    chrono::Utc::now().to_rfc3339(),
                ],
            )
            .await
            .map_err(anyhow::Error::msg)?;
        Ok(())
    }

    pub async fn revoke(
        &self,
        tenant_id: &str,
        subject_id: &str,
        capability: &str,
        source_provider: &str,
    ) -> anyhow::Result<()> {
        self.port
            .execute(
                "UPDATE commercial_entitlements SET status = 'revoked', revoked_at = ? WHERE tenant_id = ? AND subject_id = ? AND capability = ? AND source_provider = ?",
                vec![
                    chrono::Utc::now().to_rfc3339(),
                    tenant_id.to_string(),
                    subject_id.to_string(),
                    capability.to_string(),
                    source_provider.to_string(),
                ],
            )
            .await
            .map_err(anyhow::Error::msg)?;
        Ok(())
    }

    pub async fn active_for_test(
        &self,
        tenant_id: &str,
        subject_id: &str,
        capability: &str,
    ) -> anyhow::Result<bool> {
        #[derive(Deserialize)]
        struct Row {
            count: u64,
        }

        let rows = self
            .port
            .query::<Row>(
                "SELECT COUNT(*) AS count FROM commercial_entitlements WHERE tenant_id = ? AND subject_id = ? AND capability = ? AND status = 'active'",
                vec![tenant_id.to_string(), subject_id.to_string(), capability.to_string()],
            )
            .await
            .map_err(anyhow::Error::msg)?;
        Ok(rows.first().map(|row| row.count).unwrap_or(0) > 0)
    }
}

struct LibSqlEntitlementResolver {
    port: TursoBackend,
    fallback_allowed_capabilities: Vec<String>,
}

impl LibSqlEntitlementResolver {
    fn new(port: TursoBackend, fallback_allowed_capabilities: Vec<String>) -> Self {
        Self {
            port,
            fallback_allowed_capabilities,
        }
    }
}

#[async_trait]
impl EntitlementResolver for LibSqlEntitlementResolver {
    async fn check(
        &self,
        tenant: &CommercialTenant,
        subject: &CommercialSubject,
        capability: &CapabilityKey,
    ) -> Result<EntitlementDecision, ::commercial::EntitlementError> {
        #[derive(Deserialize)]
        struct Row {
            count: u64,
        }

        let rows = self
            .port
            .query::<Row>(
                "SELECT COUNT(*) AS count FROM commercial_entitlements WHERE tenant_id = ? AND subject_id = ? AND capability = ? AND status = 'active'",
                vec![
                    tenant.as_str().to_string(),
                    subject.as_str().to_string(),
                    capability.as_str().to_string(),
                ],
            )
            .await
            .map_err(|error| ::commercial::EntitlementError::Resolver(error.to_string()))?;

        if rows.first().map(|row| row.count).unwrap_or(0) > 0
            || self
                .fallback_allowed_capabilities
                .iter()
                .any(|allowed| allowed == capability.as_str())
        {
            return Ok(EntitlementDecision::Allowed);
        }

        Ok(EntitlementDecision::Denied {
            reason: format!("missing entitlement for '{}'", capability.as_str()),
        })
    }
}

impl CommercialStack {
    pub async fn set_counter_quota_limit_for_test(&self, tenant_id: &str, limit: u64) {
        let tenant = CommercialTenant::new(tenant_id);
        match &self.inspection {
            CommercialInspection::None => {}
            CommercialInspection::InMemory { quota, .. } => {
                quota
                    .set_limit(&tenant, &self.counter_paid_capability, limit)
                    .await;
            }
            CommercialInspection::LibSql { quota, .. } => {
                quota
                    .set_limit(&tenant, &self.counter_paid_capability, limit)
                    .await
                    .expect("failed to set commercial quota limit for test");
            }
        }
    }

    pub async fn usage_events_for_test(&self) -> Vec<UsageEvent> {
        match &self.inspection {
            CommercialInspection::None => Vec::new(),
            CommercialInspection::InMemory { usage, .. } => usage.events().await,
            CommercialInspection::LibSql { usage, .. } => usage
                .events()
                .await
                .expect("failed to read commercial usage events for test"),
        }
    }

    pub async fn committed_counter_usage_for_test(&self, tenant_id: &str) -> u64 {
        let tenant = CommercialTenant::new(tenant_id);
        match &self.inspection {
            CommercialInspection::None => 0,
            CommercialInspection::InMemory { quota, .. } => {
                quota
                    .committed_usage(&tenant, &self.counter_paid_capability)
                    .await
            }
            CommercialInspection::LibSql { quota, .. } => quota
                .committed_usage(&tenant, &self.counter_paid_capability)
                .await
                .expect("failed to read commercial committed usage for test"),
        }
    }
}

struct LibSqlQuotaLedger {
    port: TursoBackend,
}

impl LibSqlQuotaLedger {
    fn new(port: TursoBackend) -> Self {
        Self { port }
    }

    async fn set_limit(
        &self,
        tenant: &CommercialTenant,
        capability: &CapabilityKey,
        limit: u64,
    ) -> Result<(), ::commercial::QuotaLedgerError> {
        self.port
            .execute(
                "INSERT INTO commercial_quota_limits (tenant_id, capability, limit_value) VALUES (?, ?, ?) ON CONFLICT(tenant_id, capability) DO UPDATE SET limit_value = excluded.limit_value",
                vec![tenant.as_str().to_string(), capability.as_str().to_string(), limit.to_string()],
            )
            .await
            .map_err(|error| ::commercial::QuotaLedgerError::Ledger(error.to_string()))?;
        Ok(())
    }

    async fn committed_usage(
        &self,
        tenant: &CommercialTenant,
        capability: &CapabilityKey,
    ) -> Result<u64, ::commercial::QuotaLedgerError> {
        #[derive(Deserialize)]
        struct Row {
            quantity: u64,
        }

        let rows = self
            .port
            .query::<Row>(
                "SELECT COALESCE(SUM(quantity), 0) AS quantity FROM commercial_quota_reservations WHERE tenant_id = ? AND capability = ? AND status = 'committed'",
                vec![tenant.as_str().to_string(), capability.as_str().to_string()],
            )
            .await
            .map_err(|error| ::commercial::QuotaLedgerError::Ledger(error.to_string()))?;
        Ok(rows.first().map(|row| row.quantity).unwrap_or(0))
    }
}

#[async_trait]
impl QuotaLedger for LibSqlQuotaLedger {
    async fn reserve(
        &self,
        tenant: &CommercialTenant,
        subject: &CommercialSubject,
        capability: &CapabilityKey,
        quantity: u64,
    ) -> Result<QuotaDecision, ::commercial::QuotaLedgerError> {
        let reservation = ::commercial::QuotaReservation {
            reservation_id: uuid::Uuid::now_v7().to_string(),
            tenant: tenant.clone(),
            subject: subject.clone(),
            capability: capability.clone(),
            quantity,
            status: ::commercial::QuotaReservationStatus::Reserved,
            created_at: chrono::Utc::now(),
        };

        let changed = self
            .port
            .execute(
            "INSERT INTO commercial_quota_reservations (reservation_id, tenant_id, subject_id, capability, quantity, status, created_at) SELECT ?, ?, ?, ?, ?, ?, ? WHERE COALESCE((SELECT SUM(quantity) FROM commercial_quota_reservations WHERE tenant_id = ? AND capability = ? AND status IN ('reserved', 'committed')), 0) + CAST(? AS INTEGER) <= COALESCE((SELECT limit_value FROM commercial_quota_limits WHERE tenant_id = ? AND capability = ?), 9223372036854775807)",
            vec![
                reservation.reservation_id.clone(),
                tenant.as_str().to_string(),
                subject.as_str().to_string(),
                capability.as_str().to_string(),
                quantity.to_string(),
                "reserved".to_string(),
                reservation.created_at.to_rfc3339(),
                tenant.as_str().to_string(),
                capability.as_str().to_string(),
                quantity.to_string(),
                tenant.as_str().to_string(),
                capability.as_str().to_string(),
            ],
        )
        .await
        .map_err(|error| ::commercial::QuotaLedgerError::Ledger(error.to_string()))?;
        if changed == 0 {
            return Ok(QuotaDecision::Denied {
                reason: format!("quota exceeded for '{}'", capability.as_str()),
            });
        }

        Ok(QuotaDecision::Reserved(reservation))
    }

    async fn commit(&self, reservation_id: &str) -> Result<(), ::commercial::QuotaLedgerError> {
        let changed = self
            .port
            .execute(
                "UPDATE commercial_quota_reservations SET status = 'committed' WHERE reservation_id = ? AND status = 'reserved'",
                vec![reservation_id.to_string()],
            )
            .await
            .map_err(|error| ::commercial::QuotaLedgerError::Ledger(error.to_string()))?;
        if changed == 0 {
            return Err(::commercial::QuotaLedgerError::InvalidTransition(format!(
                "cannot commit {reservation_id}"
            )));
        }
        Ok(())
    }

    async fn release(&self, reservation_id: &str) -> Result<(), ::commercial::QuotaLedgerError> {
        let changed = self
            .port
            .execute(
                "UPDATE commercial_quota_reservations SET status = 'released' WHERE reservation_id = ? AND status = 'reserved'",
                vec![reservation_id.to_string()],
            )
            .await
            .map_err(|error| ::commercial::QuotaLedgerError::Ledger(error.to_string()))?;
        if changed == 0 {
            return Err(::commercial::QuotaLedgerError::InvalidTransition(format!(
                "cannot release {reservation_id}"
            )));
        }
        Ok(())
    }
}

struct LibSqlUsageMeter {
    port: TursoBackend,
}

impl LibSqlUsageMeter {
    fn new(port: TursoBackend) -> Self {
        Self { port }
    }

    async fn events(&self) -> Result<Vec<UsageEvent>, ::commercial::UsageMeterError> {
        #[derive(Deserialize)]
        struct Row {
            event_id: String,
            occurred_at: String,
            tenant_id: String,
            subject_id: String,
            meter_name: String,
            quantity: u64,
            unit: String,
            resource_type: String,
            resource_id: String,
            idempotency_key: Option<String>,
            metadata: String,
        }

        self.port
            .query::<Row>(
                "SELECT event_id, occurred_at, tenant_id, subject_id, meter_name, quantity, unit, resource_type, resource_id, NULLIF(idempotency_key, '') AS idempotency_key, metadata FROM commercial_usage_events ORDER BY occurred_at ASC, event_id ASC",
                vec![],
            )
            .await
            .map_err(|error| ::commercial::UsageMeterError::Meter(error.to_string()))?
            .into_iter()
            .map(|row| {
                Ok(UsageEvent {
                    event_id: row.event_id,
                    occurred_at: chrono::DateTime::parse_from_rfc3339(&row.occurred_at)
                        .map_err(|error| ::commercial::UsageMeterError::Meter(error.to_string()))?
                        .with_timezone(&chrono::Utc),
                    tenant: CommercialTenant::new(row.tenant_id),
                    subject: CommercialSubject::new(row.subject_id),
                    meter_name: row.meter_name,
                    quantity: row.quantity,
                    unit: row.unit,
                    resource_type: row.resource_type,
                    resource_id: row.resource_id,
                    idempotency_key: row.idempotency_key,
                    metadata: serde_json::from_str(&row.metadata)
                        .map_err(|error| ::commercial::UsageMeterError::Meter(error.to_string()))?,
                })
            })
            .collect()
    }
}

#[async_trait]
impl UsageMeter for LibSqlUsageMeter {
    async fn record(&self, event: UsageEvent) -> Result<(), ::commercial::UsageMeterError> {
        self.port
            .execute(
                "INSERT INTO commercial_usage_events (event_id, occurred_at, tenant_id, subject_id, meter_name, quantity, unit, resource_type, resource_id, idempotency_key, metadata) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                vec![
                    event.event_id,
                    event.occurred_at.to_rfc3339(),
                    event.tenant.as_str().to_string(),
                    event.subject.as_str().to_string(),
                    event.meter_name,
                    event.quantity.to_string(),
                    event.unit,
                    event.resource_type,
                    event.resource_id,
                    event.idempotency_key.unwrap_or_default(),
                    serde_json::to_string(&event.metadata)
                        .map_err(|error| ::commercial::UsageMeterError::Meter(error.to_string()))?,
                ],
            )
            .await
            .map_err(|error| ::commercial::UsageMeterError::Meter(error.to_string()))?;
        Ok(())
    }
}
