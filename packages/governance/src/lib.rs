//! Framework-neutral governance capability seams.
//!
//! Admin operations, product analytics, and feature flags are runtime
//! capabilities. They must not bypass service invariants or become compile-time
//! product behavior hidden behind Cargo features.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use security_audit::{AuditEvent, AuditOutcome, AuditSink, redact_metadata};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdminOperationMode {
    Disabled,
    LocalMock,
    LocalReal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdminScope {
    Global,
    Tenant(String),
}

impl AdminScope {
    fn tenant_id(&self) -> Option<&str> {
        match self {
            Self::Global => None,
            Self::Tenant(tenant_id) => Some(tenant_id),
        }
    }
}

#[derive(Debug, Clone)]
pub struct AdminOperationRequest {
    pub operation_id: String,
    pub actor_sub: String,
    pub scope: AdminScope,
    pub action: String,
    pub resource_type: String,
    pub resource_id: String,
    pub reason: Option<String>,
    pub idempotency_key: Option<String>,
    pub request_id: Option<String>,
    pub trace_id: Option<String>,
    pub metadata: serde_json::Value,
}

impl AdminOperationRequest {
    pub fn new(
        actor_sub: impl Into<String>,
        scope: AdminScope,
        action: impl Into<String>,
        resource_type: impl Into<String>,
        resource_id: impl Into<String>,
    ) -> Self {
        Self {
            operation_id: uuid::Uuid::now_v7().to_string(),
            actor_sub: actor_sub.into(),
            scope,
            action: action.into(),
            resource_type: resource_type.into(),
            resource_id: resource_id.into(),
            reason: None,
            idempotency_key: None,
            request_id: None,
            trace_id: None,
            metadata: serde_json::json!({}),
        }
    }

    pub fn reason(mut self, reason: impl Into<String>) -> Self {
        self.reason = Some(reason.into());
        self
    }

    pub fn idempotency_key(mut self, idempotency_key: impl Into<String>) -> Self {
        self.idempotency_key = Some(idempotency_key.into());
        self
    }

    pub fn request(mut self, request_id: Option<String>, trace_id: Option<String>) -> Self {
        self.request_id = request_id;
        self.trace_id = trace_id;
        self
    }

    pub fn metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = metadata;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdminOperationResponse {
    pub message: String,
}

impl AdminOperationResponse {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AdminUseCaseError {
    #[error("admin operation violated service invariant: {0}")]
    InvariantViolation(String),
    #[error("admin operation rejected: {0}")]
    Rejected(String),
}

#[async_trait]
pub trait AdminUseCasePort: Send + Sync {
    async fn execute_admin_operation(
        &self,
        request: AdminOperationRequest,
    ) -> Result<AdminOperationResponse, AdminUseCaseError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdminOperationOutcome {
    SkippedDisabled,
    Executed(AdminOperationResponse),
}

#[derive(Debug, thiserror::Error)]
pub enum AdminOperationError {
    #[error("admin operations are disabled")]
    Disabled,
    #[error(transparent)]
    UseCase(#[from] AdminUseCaseError),
    #[error("audit append failed: {0}")]
    Audit(String),
}

pub struct AdminOperationExecutor {
    mode: AdminOperationMode,
    use_case: Arc<dyn AdminUseCasePort>,
    audit: Arc<dyn AuditSink>,
}

impl AdminOperationExecutor {
    pub fn new(
        mode: AdminOperationMode,
        use_case: Arc<dyn AdminUseCasePort>,
        audit: Arc<dyn AuditSink>,
    ) -> Self {
        Self {
            mode,
            use_case,
            audit,
        }
    }

    pub async fn execute(
        &self,
        request: AdminOperationRequest,
    ) -> Result<AdminOperationOutcome, AdminOperationError> {
        if self.mode == AdminOperationMode::Disabled {
            self.append_audit(&request, AuditOutcome::Denied, Some("disabled"))
                .await?;
            return Ok(AdminOperationOutcome::SkippedDisabled);
        }

        match self.use_case.execute_admin_operation(request.clone()).await {
            Ok(response) => {
                self.append_audit(&request, AuditOutcome::Succeeded, None)
                    .await?;
                Ok(AdminOperationOutcome::Executed(response))
            }
            Err(error) => {
                let message = error.to_string();
                self.append_audit(&request, AuditOutcome::Failed, Some(&message))
                    .await?;
                Err(AdminOperationError::UseCase(error))
            }
        }
    }

    async fn append_audit(
        &self,
        request: &AdminOperationRequest,
        outcome: AuditOutcome,
        error: Option<&str>,
    ) -> Result<(), AdminOperationError> {
        let mut metadata = serde_json::json!({
            "operation_id": request.operation_id,
            "reason": request.reason,
            "idempotency_key": request.idempotency_key,
            "admin_metadata": request.metadata,
        });
        if let Some(error) = error {
            metadata["error"] = serde_json::Value::String(error.to_string());
        }

        let mut event = AuditEvent::new(
            format!("admin.{}", request.action),
            request.resource_type.clone(),
            request.resource_id.clone(),
            outcome,
        )
        .actor(request.actor_sub.clone())
        .request(request.request_id.clone(), request.trace_id.clone())
        .metadata(metadata);
        if let Some(tenant_id) = request.scope.tenant_id() {
            event = event.tenant(tenant_id.to_string());
        }

        self.audit
            .append(event)
            .await
            .map_err(|error| AdminOperationError::Audit(error.to_string()))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProductEventMode {
    Disabled,
    LocalMock,
    LocalReal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductEvent {
    pub event_id: String,
    pub occurred_at: DateTime<Utc>,
    pub actor_sub: Option<String>,
    pub tenant_id: Option<String>,
    pub event_name: String,
    pub resource_type: String,
    pub resource_id: String,
    pub request_id: Option<String>,
    pub trace_id: Option<String>,
    pub properties: serde_json::Value,
}

impl ProductEvent {
    pub fn new(
        event_name: impl Into<String>,
        resource_type: impl Into<String>,
        resource_id: impl Into<String>,
    ) -> Self {
        Self {
            event_id: uuid::Uuid::now_v7().to_string(),
            occurred_at: Utc::now(),
            actor_sub: None,
            tenant_id: None,
            event_name: event_name.into(),
            resource_type: resource_type.into(),
            resource_id: resource_id.into(),
            request_id: None,
            trace_id: None,
            properties: serde_json::json!({}),
        }
    }

    pub fn actor(mut self, actor_sub: impl Into<String>) -> Self {
        self.actor_sub = Some(actor_sub.into());
        self
    }

    pub fn tenant(mut self, tenant_id: impl Into<String>) -> Self {
        self.tenant_id = Some(tenant_id.into());
        self
    }

    pub fn request(mut self, request_id: Option<String>, trace_id: Option<String>) -> Self {
        self.request_id = request_id;
        self.trace_id = trace_id;
        self
    }

    pub fn properties(mut self, properties: serde_json::Value) -> Self {
        self.properties = redact_metadata(properties);
        self
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ProductEventError {
    #[error("product event recorder error: {0}")]
    Recorder(String),
}

#[async_trait]
pub trait ProductEventRecorder: Send + Sync {
    async fn record(&self, event: ProductEvent) -> Result<(), ProductEventError>;
}

#[derive(Debug, Default)]
pub struct InMemoryProductEventRecorder {
    events: tokio::sync::Mutex<Vec<ProductEvent>>,
}

impl InMemoryProductEventRecorder {
    pub fn shared() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub async fn events(&self) -> Vec<ProductEvent> {
        self.events.lock().await.clone()
    }
}

#[async_trait]
impl ProductEventRecorder for InMemoryProductEventRecorder {
    async fn record(&self, event: ProductEvent) -> Result<(), ProductEventError> {
        self.events.lock().await.push(event);
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProductEventOutcome {
    SkippedDisabled,
    Recorded,
}

pub struct ProductEventService {
    mode: ProductEventMode,
    recorder: Arc<dyn ProductEventRecorder>,
}

impl ProductEventService {
    pub fn new(mode: ProductEventMode, recorder: Arc<dyn ProductEventRecorder>) -> Self {
        Self { mode, recorder }
    }

    pub async fn record(
        &self,
        event: ProductEvent,
    ) -> Result<ProductEventOutcome, ProductEventError> {
        if self.mode == ProductEventMode::Disabled {
            return Ok(ProductEventOutcome::SkippedDisabled);
        }

        self.recorder.record(event).await?;
        Ok(ProductEventOutcome::Recorded)
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use security_audit::InMemoryAuditSink;

    #[derive(Debug, Default)]
    struct CounterCorrectionUseCase {
        value: tokio::sync::Mutex<i64>,
    }

    #[async_trait]
    impl AdminUseCasePort for CounterCorrectionUseCase {
        async fn execute_admin_operation(
            &self,
            request: AdminOperationRequest,
        ) -> Result<AdminOperationResponse, AdminUseCaseError> {
            let target = request
                .metadata
                .get("target_value")
                .and_then(serde_json::Value::as_i64)
                .unwrap_or(0);
            if target < 0 {
                return Err(AdminUseCaseError::InvariantViolation(
                    "counter value cannot be negative".to_string(),
                ));
            }

            *self.value.lock().await = target;
            Ok(AdminOperationResponse::new("counter corrected"))
        }
    }

    #[tokio::test]
    async fn admin_operation_delegates_to_use_case_and_records_redacted_audit_shape() {
        let use_case = Arc::new(CounterCorrectionUseCase::default());
        let audit = InMemoryAuditSink::shared();
        let executor = AdminOperationExecutor::new(
            AdminOperationMode::LocalMock,
            use_case.clone(),
            audit.clone(),
        );

        let outcome = executor
            .execute(
                AdminOperationRequest::new(
                    "admin-user",
                    AdminScope::Tenant("tenant-a".to_string()),
                    "counter.correct",
                    "counter",
                    "tenant-a",
                )
                .reason("support correction")
                .idempotency_key("raw-idempotency-key")
                .request(Some("req-1".to_string()), Some("trace-1".to_string()))
                .metadata(serde_json::json!({
                    "target_value": 3,
                    "authorization": "Bearer raw-token"
                })),
            )
            .await
            .unwrap();

        assert!(matches!(outcome, AdminOperationOutcome::Executed(_)));
        assert_eq!(*use_case.value.lock().await, 3);

        let events = audit.events().await;
        assert_eq!(events.len(), 1);
        let event = &events[0];
        assert_eq!(event.actor_sub.as_deref(), Some("admin-user"));
        assert_eq!(event.tenant_id.as_deref(), Some("tenant-a"));
        assert_eq!(event.action, "admin.counter.correct");
        assert_eq!(event.resource_type, "counter");
        assert_eq!(event.resource_id, "tenant-a");
        assert_eq!(event.outcome, AuditOutcome::Succeeded);
        assert_eq!(event.request_id.as_deref(), Some("req-1"));
        assert_eq!(event.trace_id.as_deref(), Some("trace-1"));
        assert_eq!(event.metadata["idempotency_key"], "[redacted]");
        assert_eq!(
            event.metadata["admin_metadata"]["authorization"],
            "[redacted]"
        );
    }

    #[tokio::test]
    async fn admin_operation_preserves_service_invariant_on_rejected_mutation() {
        let use_case = Arc::new(CounterCorrectionUseCase::default());
        let audit = InMemoryAuditSink::shared();
        let executor = AdminOperationExecutor::new(
            AdminOperationMode::LocalMock,
            use_case.clone(),
            audit.clone(),
        );

        let result = executor
            .execute(
                AdminOperationRequest::new(
                    "admin-user",
                    AdminScope::Tenant("tenant-a".to_string()),
                    "counter.correct",
                    "counter",
                    "tenant-a",
                )
                .metadata(serde_json::json!({ "target_value": -1 })),
            )
            .await;

        assert!(matches!(
            result,
            Err(AdminOperationError::UseCase(
                AdminUseCaseError::InvariantViolation(_)
            ))
        ));
        assert_eq!(*use_case.value.lock().await, 0);
        let events = audit.events().await;
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].outcome, AuditOutcome::Failed);
    }

    #[tokio::test]
    async fn disabled_admin_operation_skips_use_case_and_records_denial() {
        let use_case = Arc::new(CounterCorrectionUseCase::default());
        let audit = InMemoryAuditSink::shared();
        let executor = AdminOperationExecutor::new(
            AdminOperationMode::Disabled,
            use_case.clone(),
            audit.clone(),
        );

        let outcome = executor
            .execute(
                AdminOperationRequest::new(
                    "admin-user",
                    AdminScope::Global,
                    "counter.correct",
                    "counter",
                    "global",
                )
                .metadata(serde_json::json!({ "target_value": 5 })),
            )
            .await
            .unwrap();

        assert_eq!(outcome, AdminOperationOutcome::SkippedDisabled);
        assert_eq!(*use_case.value.lock().await, 0);
        let events = audit.events().await;
        assert_eq!(events[0].outcome, AuditOutcome::Denied);
    }

    #[tokio::test]
    async fn product_events_are_server_side_runtime_events_with_redacted_properties() {
        let recorder = InMemoryProductEventRecorder::shared();
        let service = ProductEventService::new(ProductEventMode::LocalMock, recorder.clone());

        let outcome = service
            .record(
                ProductEvent::new("counter.incremented", "counter", "tenant-a")
                    .actor("user-a")
                    .tenant("tenant-a")
                    .request(Some("req-1".to_string()), Some("trace-1".to_string()))
                    .properties(serde_json::json!({
                        "source": "web-bff",
                        "jwt": "raw-jwt"
                    })),
            )
            .await
            .unwrap();

        assert_eq!(outcome, ProductEventOutcome::Recorded);
        let events = recorder.events().await;
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_name, "counter.incremented");
        assert_eq!(events[0].actor_sub.as_deref(), Some("user-a"));
        assert_eq!(events[0].properties["source"], "web-bff");
        assert_eq!(events[0].properties["jwt"], "[redacted]");
    }

    #[tokio::test]
    async fn disabled_product_events_do_not_record() {
        let recorder = InMemoryProductEventRecorder::shared();
        let service = ProductEventService::new(ProductEventMode::Disabled, recorder.clone());

        let outcome = service
            .record(ProductEvent::new(
                "counter.incremented",
                "counter",
                "tenant-a",
            ))
            .await
            .unwrap();

        assert_eq!(outcome, ProductEventOutcome::SkippedDisabled);
        assert!(recorder.events().await.is_empty());
    }

    #[tokio::test]
    async fn feature_flags_are_runtime_decisions_not_cargo_features() {
        let provider =
            RuntimeFeatureFlagProvider::db_backed(std::iter::empty::<(&str, FeatureFlagRule)>());
        let key = FeatureFlagKey::new("admin.bulk-correction");
        let subject = FeatureFlagSubject {
            actor_sub: Some("admin-user".to_string()),
            tenant_id: Some("tenant-a".to_string()),
            attributes: HashMap::new(),
        };

        let first = provider.evaluate(&key, &subject, false).await.unwrap();
        assert!(!first.enabled);
        assert_eq!(first.reason, "default");

        provider
            .set_flag(key.as_str(), FeatureFlagRule::enabled().variant("beta"))
            .await
            .unwrap();

        let second = provider.evaluate(&key, &subject, false).await.unwrap();
        assert!(second.enabled);
        assert_eq!(second.variant.as_deref(), Some("beta"));
        assert_eq!(second.reason, "runtime Db rule");
    }

    #[tokio::test]
    async fn config_feature_flags_are_read_only_runtime_configuration() {
        let provider = RuntimeFeatureFlagProvider::config_flags([(
            "analytics.product-events",
            FeatureFlagRule::enabled(),
        )]);
        let key = FeatureFlagKey::new("analytics.product-events");
        let subject = FeatureFlagSubject::default();

        let decision = provider.evaluate(&key, &subject, false).await.unwrap();
        assert!(decision.enabled);

        let update = provider
            .set_flag(key.as_str(), FeatureFlagRule::disabled())
            .await;
        assert!(matches!(
            update,
            Err(FeatureFlagError::ReadOnly(FeatureFlagMode::Config))
        ));
    }
}
