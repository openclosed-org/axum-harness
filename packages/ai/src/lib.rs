//! Framework-neutral AI capability seams.
//!
//! AI providers, prompt selection, tool permission, token metering, quota, and
//! trace/replay are runtime capabilities. Product code must not call provider
//! SDKs directly or hide paid AI behavior behind compile-time features.

pub mod invocation;
pub mod prompt;
pub mod provider;
pub mod tool;
pub mod trace;

pub use invocation::{
    AiInvocationError, AiInvocationRequest, AiInvocationResponse, AiInvocationService,
};
pub use prompt::{InMemoryPromptRegistry, PromptRegistry, PromptRegistryError, PromptVersion};
pub use provider::{
    LlmProviderError, LlmProviderPort, LlmProviderRequest, LlmProviderResponse, MockLlmProvider,
};
pub use tool::{
    StaticToolPermissionPolicy, ToolKey, ToolPermissionDecision, ToolPermissionError,
    ToolPermissionPolicy,
};
pub use trace::{
    AiTraceEvent, AiTraceOutcome, AiTraceStore, AiTraceStoreError, InMemoryAiTraceStore,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiCapabilityMode {
    Disabled,
    LocalMock,
    LocalReal,
    ExternalSingleNode,
}

#[cfg(test)]
mod tests {
    use super::*;
    use commercial::{
        CapabilityKey, CommercialSubject, CommercialTenant, EntitlementResolver,
        InMemoryQuotaLedger, InMemoryUsageMeter, StaticEntitlementResolver,
    };
    use std::sync::Arc;

    struct Harness {
        service: AiInvocationService,
        provider: Arc<MockLlmProvider>,
        prompts: Arc<InMemoryPromptRegistry>,
        quota: Arc<InMemoryQuotaLedger>,
        usage: Arc<InMemoryUsageMeter>,
        traces: Arc<InMemoryAiTraceStore>,
    }

    async fn harness(entitled: bool, allowed_tools: &[&str]) -> Harness {
        let tenant = CommercialTenant::new("tenant-a");
        let capability = CapabilityKey::new("llm.invoke");
        let prompts = InMemoryPromptRegistry::shared();
        prompts
            .register(PromptVersion::new(
                "assistant.answer",
                "v1",
                "You are concise.",
            ))
            .await;
        prompts
            .register(PromptVersion::new(
                "assistant.answer",
                "v2",
                "You are precise.",
            ))
            .await;

        let quota = Arc::new(InMemoryQuotaLedger::default());
        quota.set_limit(&tenant, &capability, 100).await;
        let usage = Arc::new(InMemoryUsageMeter::default());
        let provider = MockLlmProvider::shared(LlmProviderResponse {
            provider_request_id: "mock-request-1".to_string(),
            output_text: "mocked answer".to_string(),
            input_tokens: 12,
            output_tokens: 8,
            finish_reason: "stop".to_string(),
        });
        let traces = InMemoryAiTraceStore::shared();
        let entitlement = if entitled {
            Arc::new(StaticEntitlementResolver::allow_list(["llm.invoke"]))
                as Arc<dyn EntitlementResolver>
        } else {
            Arc::new(StaticEntitlementResolver::allow_list(["counter.write"]))
                as Arc<dyn EntitlementResolver>
        };
        let tools = Arc::new(StaticToolPermissionPolicy::allow_tools(
            allowed_tools.iter().copied(),
        ));

        let service = AiInvocationService::new(
            AiCapabilityMode::LocalMock,
            entitlement,
            quota.clone(),
            usage.clone(),
            prompts.clone(),
            tools,
            provider.clone(),
            traces.clone(),
        );

        Harness {
            service,
            provider,
            prompts,
            quota,
            usage,
            traces,
        }
    }

    fn request(max_tokens: u64) -> AiInvocationRequest {
        AiInvocationRequest::new(
            CommercialTenant::new("tenant-a"),
            CommercialSubject::new("user-a"),
            CapabilityKey::new("llm.invoke"),
            "assistant.answer",
            "mock-model",
            "Explain counters.",
            max_tokens,
        )
        .tools([ToolKey::new("counter.read")])
        .idempotency_key("raw-idempotency-key")
        .metadata(serde_json::json!({
            "source": "unit-test",
            "authorization": "Bearer raw-token"
        }))
    }

    #[tokio::test]
    async fn provider_adapter_contract_uses_mocked_provider_and_selected_prompt() {
        let harness = harness(true, &["counter.read"]).await;

        let response = harness
            .service
            .invoke(request(50).prompt_version("v1"))
            .await
            .unwrap();

        assert_eq!(response.provider_response.output_text, "mocked answer");
        assert_eq!(response.prompt_version.version, "v1");
        let requests = harness.provider.requests().await;
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].model, "mock-model");
        assert_eq!(requests[0].system_prompt, "You are concise.");
        assert_eq!(requests[0].tools[0].as_str(), "counter.read");
    }

    #[tokio::test]
    async fn token_metering_records_actual_provider_tokens() {
        let harness = harness(true, &["counter.read"]).await;

        harness.service.invoke(request(50)).await.unwrap();

        let events = harness.usage.events().await;
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].meter_name, "llm.tokens");
        assert_eq!(events[0].quantity, 20);
        assert_eq!(events[0].unit, "token");
        assert_eq!(events[0].metadata["prompt_version"], "v2");
    }

    #[tokio::test]
    async fn quota_denial_skips_provider_and_usage_metering() {
        let harness = harness(true, &["counter.read"]).await;
        harness
            .quota
            .set_limit(
                &CommercialTenant::new("tenant-a"),
                &CapabilityKey::new("llm.invoke"),
                10,
            )
            .await;

        let error = harness.service.invoke(request(50)).await.unwrap_err();

        assert!(matches!(error, AiInvocationError::QuotaDenied(_)));
        assert!(harness.provider.requests().await.is_empty());
        assert!(harness.usage.events().await.is_empty());
        let traces = harness.traces.events().await;
        assert_eq!(traces[0].outcome, AiTraceOutcome::Denied);
        assert_eq!(traces[0].prompt_version.as_deref(), Some("v2"));
    }

    #[tokio::test]
    async fn prompt_registry_selects_requested_version_or_latest_default() {
        let harness = harness(true, &["counter.read"]).await;

        let explicit = harness
            .prompts
            .select_prompt("assistant.answer", Some("v1"))
            .await
            .unwrap();
        let latest = harness
            .prompts
            .select_prompt("assistant.answer", None)
            .await
            .unwrap();

        assert_eq!(explicit.version, "v1");
        assert_eq!(latest.version, "v2");
    }

    #[tokio::test]
    async fn tool_permission_denial_skips_provider() {
        let harness = harness(true, &["billing.lookup"]).await;

        let error = harness.service.invoke(request(50)).await.unwrap_err();

        assert!(matches!(error, AiInvocationError::ToolDenied(_)));
        assert!(harness.provider.requests().await.is_empty());
        let traces = harness.traces.events().await;
        assert_eq!(traces[0].outcome, AiTraceOutcome::Denied);
        assert!(traces[0].error.as_deref().unwrap().contains("counter.read"));
    }

    #[tokio::test]
    async fn entitlement_denial_skips_provider() {
        let harness = harness(false, &["counter.read"]).await;

        let error = harness.service.invoke(request(50)).await.unwrap_err();

        assert!(matches!(error, AiInvocationError::EntitlementDenied(_)));
        assert!(harness.provider.requests().await.is_empty());
        assert!(harness.usage.events().await.is_empty());
    }

    #[tokio::test]
    async fn trace_replay_shape_records_redacted_success_metadata() {
        let harness = harness(true, &["counter.read"]).await;

        harness.service.invoke(request(50)).await.unwrap();

        let traces = harness.traces.events().await;
        assert_eq!(traces.len(), 1);
        assert_eq!(traces[0].outcome, AiTraceOutcome::Succeeded);
        assert_eq!(
            traces[0].provider_request_id.as_deref(),
            Some("mock-request-1")
        );
        assert_eq!(traces[0].input_tokens, 12);
        assert_eq!(traces[0].output_tokens, 8);
        assert_eq!(traces[0].metadata["source"], "unit-test");
        assert_eq!(traces[0].metadata["authorization"], "[redacted]");
    }
}
