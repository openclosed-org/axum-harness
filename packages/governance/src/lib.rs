//! Framework-neutral governance capability seams.
//!
//! Admin operations, product analytics, and feature flags are runtime
//! capabilities. They must not bypass service invariants or become compile-time
//! product behavior hidden behind Cargo features.

pub mod admin;
pub mod analytics;
pub mod feature_flags;

pub use admin::{
    AdminOperationError, AdminOperationExecutor, AdminOperationMode, AdminOperationOutcome,
    AdminOperationRequest, AdminOperationResponse, AdminScope, AdminUseCaseError, AdminUseCasePort,
};
pub use analytics::{
    InMemoryProductEventRecorder, ProductEvent, ProductEventError, ProductEventMode,
    ProductEventOutcome, ProductEventRecorder, ProductEventService,
};
pub use feature_flags::{
    FeatureFlagDecision, FeatureFlagError, FeatureFlagKey, FeatureFlagMode, FeatureFlagProvider,
    FeatureFlagRule, FeatureFlagSubject, RuntimeFeatureFlagProvider,
};

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use security_audit::{AuditOutcome, InMemoryAuditSink};
    use std::collections::HashMap;
    use std::sync::Arc;

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
