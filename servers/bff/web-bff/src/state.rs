//! BFF 状态 — 组合所有服务依赖并注入 Axum State。
//!
//! Supports both embedded Turso (local SQLite) and remote Turso cloud.
//! When `turso_url` is configured, connects to Turso cloud.
//! Otherwise falls back to embedded database.

use crate::audit::{AuditEvent, InMemoryAuditSink};
use crate::bootstrap::{bootstrap_bff_state, bootstrap_test_state};
use crate::composition::{
    CounterServiceHandle, TenantServiceHandle, UserProfileRepositoryHandle,
    UserTenantInfoRepositoryHandle, UserTenantRepositoryHandle,
};
use crate::config::Config;
use authn_oidc_verifier::OidcVerifier;
use authz::{AuthzPort, AuthzTupleKey};
use moka::future::Cache;
use std::sync::Arc;
use storage_turso::{EmbeddedTurso, TursoCloud};

/// Database backend — either embedded or remote Turso.
#[derive(Clone)]
pub enum DatabaseBackend {
    Embedded(EmbeddedTurso),
    Remote(TursoCloud),
}

/// Web BFF 应用状态。
///
/// All fields are cheaply cloneable (Arc-wrapped internally where needed).
#[derive(Clone)]
pub struct BffState {
    pub(crate) config: Config,

    /// Database backend — embedded or remote Turso.
    pub(crate) db: Option<DatabaseBackend>,

    /// Prebuilt application wiring owned by the Web BFF composition root.
    pub(crate) composition: Option<BffCompositionRoot>,

    /// In-process cache for counter values (tenant_id → value).
    pub(crate) counter_cache: Cache<String, i64>,

    /// Shared HTTP client for external service calls.
    pub(crate) http_client: reqwest::Client,

    /// Authorization adapter — mock for dev, OpenFGA when explicitly configured.
    pub(crate) authz: Arc<dyn AuthzPort>,

    /// Shared OIDC verifier. Present only when generic OIDC issuer is configured.
    pub(crate) oidc_verifier: Option<OidcVerifier>,

    /// Append-only BFF boundary audit sink.
    pub(crate) audit: Arc<InMemoryAuditSink>,
}

#[derive(Clone)]
pub struct BffCompositionRoot {
    pub(crate) counter_service: CounterServiceHandle,
    pub(crate) tenant_service: TenantServiceHandle,
    pub(crate) user_profile_repository: UserProfileRepositoryHandle,
    pub(crate) user_tenant_repository: UserTenantRepositoryHandle,
    pub(crate) user_tenant_info_repository: UserTenantInfoRepositoryHandle,
}

impl BffState {
    pub async fn new(config: Config) -> anyhow::Result<Self> {
        bootstrap_bff_state(config).await
    }

    /// Create BffState with a pre-initialized database instance.
    /// Used for testing with in-memory databases.
    pub async fn new_with_db(db: EmbeddedTurso) -> Self {
        bootstrap_test_state(db)
            .await
            .expect("failed to bootstrap test BFF state")
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    pub fn config_mut(&mut self) -> &mut Config {
        &mut self.config
    }

    pub fn http_client(&self) -> reqwest::Client {
        self.http_client.clone()
    }

    pub fn authz(&self) -> Arc<dyn AuthzPort> {
        self.authz.clone()
    }

    pub fn oidc_verifier(&self) -> Option<OidcVerifier> {
        self.oidc_verifier.clone()
    }

    pub fn set_oidc_verifier(&mut self, verifier: Option<OidcVerifier>) {
        self.oidc_verifier = verifier;
    }

    pub async fn append_audit(&self, event: AuditEvent) {
        self.audit.append(event).await;
    }

    pub async fn audit_events(&self) -> Vec<AuditEvent> {
        self.audit.events().await
    }

    pub fn counter_cache(&self) -> &Cache<String, i64> {
        &self.counter_cache
    }

    pub fn clear_database_and_composition_for_test(&mut self) {
        self.db = None;
        self.composition = None;
    }

    pub fn readiness(&self) -> ReadinessStatus {
        let mut unavailable = Vec::new();
        if self.db.is_none() {
            unavailable.push("database".to_string());
        }
        if self.composition.is_none() {
            unavailable.push("composition".to_string());
        }

        ReadinessStatus { unavailable }
    }

    pub async fn seed_dev_counter_authz(
        &self,
        user_sub: &str,
        tenant_id: &str,
        role: &str,
    ) -> Result<(), authz::AuthzError> {
        let user = format!("user:{user_sub}");
        let tenant_object = format!("tenant:{tenant_id}");
        let counter_object = format!("counter:{tenant_id}");

        for tuple in [
            AuthzTupleKey::new(&user, role, &tenant_object),
            AuthzTupleKey::new(&user, "member", &tenant_object),
            AuthzTupleKey::new(&user, "can_read", &counter_object),
            AuthzTupleKey::new(&user, "can_write", &counter_object),
        ] {
            let exists = self
                .authz
                .check(&tuple.user, &tuple.relation, &tuple.object)
                .await?;
            if !exists {
                self.authz.write_tuple(&tuple).await?;
            }
        }

        Ok(())
    }

    pub fn counter_service(&self) -> Option<CounterServiceHandle> {
        self.composition
            .as_ref()
            .map(|composition| composition.counter_service.clone())
    }

    pub fn user_profile_repository(&self) -> Option<UserProfileRepositoryHandle> {
        self.composition
            .as_ref()
            .map(|composition| composition.user_profile_repository.clone())
    }

    pub fn user_tenant_repository(&self) -> Option<UserTenantRepositoryHandle> {
        self.composition
            .as_ref()
            .map(|composition| composition.user_tenant_repository.clone())
    }

    pub fn user_read_repositories(
        &self,
    ) -> Option<(UserTenantRepositoryHandle, UserTenantInfoRepositoryHandle)> {
        let composition = self.composition.as_ref()?;
        Some((
            composition.user_tenant_repository.clone(),
            composition.user_tenant_info_repository.clone(),
        ))
    }

    pub fn tenant_service(&self) -> Option<TenantServiceHandle> {
        self.composition
            .as_ref()
            .map(|composition| composition.tenant_service.clone())
    }
}

pub struct ReadinessStatus {
    unavailable: Vec<String>,
}

impl ReadinessStatus {
    pub fn is_ready(&self) -> bool {
        self.unavailable.is_empty()
    }

    pub fn unavailable(&self) -> &[String] {
        &self.unavailable
    }
}
