//! Framework-neutral AI capability seams.
//!
//! AI providers, prompt selection, tool permission, token metering, quota, and
//! trace/replay are runtime capabilities. Product code must not call provider
//! SDKs directly or hide paid AI behavior behind compile-time features.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use commercial::{
    CapabilityKey, CommercialSubject, CommercialTenant, EntitlementDecision, EntitlementError,
    EntitlementResolver, QuotaDecision, QuotaLedger, QuotaLedgerError, UsageEvent, UsageMeter,
    UsageMeterError,
};
use security_audit::redact_metadata;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiCapabilityMode {
    Disabled,
    LocalMock,
    LocalReal,
    ExternalSingleNode,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ToolKey(String);

impl ToolKey {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for ToolKey {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptVersion {
    pub prompt_key: String,
    pub version: String,
    pub template: String,
}

impl PromptVersion {
    pub fn new(
        prompt_key: impl Into<String>,
        version: impl Into<String>,
        template: impl Into<String>,
    ) -> Self {
        Self {
            prompt_key: prompt_key.into(),
            version: version.into(),
            template: template.into(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PromptRegistryError {
    #[error("prompt not found: {0}")]
    NotFound(String),
    #[error("prompt registry error: {0}")]
    Registry(String),
}

#[async_trait]
pub trait PromptRegistry: Send + Sync {
    async fn select_prompt(
        &self,
        prompt_key: &str,
        version: Option<&str>,
    ) -> Result<PromptVersion, PromptRegistryError>;
}

#[derive(Debug, Default)]
pub struct InMemoryPromptRegistry {
    prompts: tokio::sync::Mutex<HashMap<String, Vec<PromptVersion>>>,
}

impl InMemoryPromptRegistry {
    pub fn shared() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub async fn register(&self, prompt: PromptVersion) {
        self.prompts
            .lock()
            .await
            .entry(prompt.prompt_key.clone())
            .or_default()
            .push(prompt);
    }
}

#[async_trait]
impl PromptRegistry for InMemoryPromptRegistry {
    async fn select_prompt(
        &self,
        prompt_key: &str,
        version: Option<&str>,
    ) -> Result<PromptVersion, PromptRegistryError> {
        let prompts = self.prompts.lock().await;
        let versions = prompts
            .get(prompt_key)
            .ok_or_else(|| PromptRegistryError::NotFound(prompt_key.to_string()))?;

        if let Some(version) = version {
            return versions
                .iter()
                .find(|prompt| prompt.version == version)
                .cloned()
                .ok_or_else(|| PromptRegistryError::NotFound(format!("{prompt_key}@{version}")));
        }

        versions
            .last()
            .cloned()
            .ok_or_else(|| PromptRegistryError::NotFound(prompt_key.to_string()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolPermissionDecision {
    Allowed,
    Denied { reason: String },
}

#[derive(Debug, thiserror::Error)]
pub enum ToolPermissionError {
    #[error("tool permission error: {0}")]
    Policy(String),
}

#[async_trait]
pub trait ToolPermissionPolicy: Send + Sync {
    async fn check_tool(
        &self,
        tenant: &CommercialTenant,
        subject: &CommercialSubject,
        tool: &ToolKey,
    ) -> Result<ToolPermissionDecision, ToolPermissionError>;
}

#[derive(Debug, Clone, Default)]
pub struct StaticToolPermissionPolicy {
    allowed_tools: Arc<HashSet<String>>,
}

impl StaticToolPermissionPolicy {
    pub fn allow_tools(tools: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            allowed_tools: Arc::new(tools.into_iter().map(Into::into).collect()),
        }
    }
}

#[async_trait]
impl ToolPermissionPolicy for StaticToolPermissionPolicy {
    async fn check_tool(
        &self,
        _tenant: &CommercialTenant,
        _subject: &CommercialSubject,
        tool: &ToolKey,
    ) -> Result<ToolPermissionDecision, ToolPermissionError> {
        if self.allowed_tools.contains(tool.as_str()) {
            Ok(ToolPermissionDecision::Allowed)
        } else {
            Ok(ToolPermissionDecision::Denied {
                reason: format!("tool '{}' is not allowed", tool.as_str()),
            })
        }
    }
}

#[derive(Debug, Clone)]
pub struct AiInvocationRequest {
    pub invocation_id: String,
    pub tenant: CommercialTenant,
    pub subject: CommercialSubject,
    pub capability: CapabilityKey,
    pub prompt_key: String,
    pub prompt_version: Option<String>,
    pub model: String,
    pub user_input: String,
    pub requested_tools: Vec<ToolKey>,
    pub max_tokens: u64,
    pub idempotency_key: Option<String>,
    pub metadata: serde_json::Value,
}

impl AiInvocationRequest {
    pub fn new(
        tenant: CommercialTenant,
        subject: CommercialSubject,
        capability: CapabilityKey,
        prompt_key: impl Into<String>,
        model: impl Into<String>,
        user_input: impl Into<String>,
        max_tokens: u64,
    ) -> Self {
        Self {
            invocation_id: uuid::Uuid::now_v7().to_string(),
            tenant,
            subject,
            capability,
            prompt_key: prompt_key.into(),
            prompt_version: None,
            model: model.into(),
            user_input: user_input.into(),
            requested_tools: Vec::new(),
            max_tokens,
            idempotency_key: None,
            metadata: serde_json::json!({}),
        }
    }

    pub fn prompt_version(mut self, version: impl Into<String>) -> Self {
        self.prompt_version = Some(version.into());
        self
    }

    pub fn tools(mut self, tools: impl IntoIterator<Item = ToolKey>) -> Self {
        self.requested_tools = tools.into_iter().collect();
        self
    }

    pub fn idempotency_key(mut self, key: impl Into<String>) -> Self {
        self.idempotency_key = Some(key.into());
        self
    }

    pub fn metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = metadata;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LlmProviderRequest {
    pub invocation_id: String,
    pub model: String,
    pub system_prompt: String,
    pub user_input: String,
    pub tools: Vec<ToolKey>,
    pub max_tokens: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LlmProviderResponse {
    pub provider_request_id: String,
    pub output_text: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub finish_reason: String,
}

impl LlmProviderResponse {
    pub fn total_tokens(&self) -> u64 {
        self.input_tokens + self.output_tokens
    }
}

#[derive(Debug, thiserror::Error)]
pub enum LlmProviderError {
    #[error("llm provider error: {0}")]
    Provider(String),
}

#[async_trait]
pub trait LlmProviderPort: Send + Sync {
    async fn complete(
        &self,
        request: LlmProviderRequest,
    ) -> Result<LlmProviderResponse, LlmProviderError>;
}

#[derive(Debug)]
pub struct MockLlmProvider {
    response: LlmProviderResponse,
    requests: tokio::sync::Mutex<Vec<LlmProviderRequest>>,
}

impl MockLlmProvider {
    pub fn shared(response: LlmProviderResponse) -> Arc<Self> {
        Arc::new(Self {
            response,
            requests: tokio::sync::Mutex::new(Vec::new()),
        })
    }

    pub async fn requests(&self) -> Vec<LlmProviderRequest> {
        self.requests.lock().await.clone()
    }
}

#[async_trait]
impl LlmProviderPort for MockLlmProvider {
    async fn complete(
        &self,
        request: LlmProviderRequest,
    ) -> Result<LlmProviderResponse, LlmProviderError> {
        self.requests.lock().await.push(request);
        Ok(self.response.clone())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiInvocationResponse {
    pub invocation_id: String,
    pub prompt_version: PromptVersion,
    pub provider_response: LlmProviderResponse,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AiTraceOutcome {
    Succeeded,
    Denied,
    Failed,
}

#[derive(Debug, Clone)]
pub struct AiTraceEvent {
    pub trace_id: String,
    pub occurred_at: DateTime<Utc>,
    pub invocation_id: String,
    pub tenant: CommercialTenant,
    pub subject: CommercialSubject,
    pub capability: CapabilityKey,
    pub prompt_key: String,
    pub prompt_version: Option<String>,
    pub provider_request_id: Option<String>,
    pub model: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub outcome: AiTraceOutcome,
    pub error: Option<String>,
    pub metadata: serde_json::Value,
}

#[derive(Debug, thiserror::Error)]
pub enum AiTraceStoreError {
    #[error("ai trace store error: {0}")]
    Store(String),
}

#[async_trait]
pub trait AiTraceStore: Send + Sync {
    async fn append(&self, event: AiTraceEvent) -> Result<(), AiTraceStoreError>;
}

#[derive(Debug, Default)]
pub struct InMemoryAiTraceStore {
    events: tokio::sync::Mutex<Vec<AiTraceEvent>>,
}

impl InMemoryAiTraceStore {
    pub fn shared() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub async fn events(&self) -> Vec<AiTraceEvent> {
        self.events.lock().await.clone()
    }
}

#[async_trait]
impl AiTraceStore for InMemoryAiTraceStore {
    async fn append(&self, event: AiTraceEvent) -> Result<(), AiTraceStoreError> {
        self.events.lock().await.push(event);
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AiInvocationError {
    #[error("ai invocation is disabled")]
    Disabled,
    #[error("entitlement denied: {0}")]
    EntitlementDenied(String),
    #[error(transparent)]
    Entitlement(#[from] EntitlementError),
    #[error("quota denied: {0}")]
    QuotaDenied(String),
    #[error(transparent)]
    Quota(#[from] QuotaLedgerError),
    #[error("tool permission denied: {0}")]
    ToolDenied(String),
    #[error(transparent)]
    ToolPermission(#[from] ToolPermissionError),
    #[error(transparent)]
    Prompt(#[from] PromptRegistryError),
    #[error(transparent)]
    Provider(#[from] LlmProviderError),
    #[error(transparent)]
    Usage(#[from] UsageMeterError),
    #[error(transparent)]
    Trace(#[from] AiTraceStoreError),
}

pub struct AiInvocationService {
    mode: AiCapabilityMode,
    entitlement: Arc<dyn EntitlementResolver>,
    quota: Arc<dyn QuotaLedger>,
    usage: Arc<dyn UsageMeter>,
    prompts: Arc<dyn PromptRegistry>,
    tools: Arc<dyn ToolPermissionPolicy>,
    provider: Arc<dyn LlmProviderPort>,
    traces: Arc<dyn AiTraceStore>,
}

impl AiInvocationService {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        mode: AiCapabilityMode,
        entitlement: Arc<dyn EntitlementResolver>,
        quota: Arc<dyn QuotaLedger>,
        usage: Arc<dyn UsageMeter>,
        prompts: Arc<dyn PromptRegistry>,
        tools: Arc<dyn ToolPermissionPolicy>,
        provider: Arc<dyn LlmProviderPort>,
        traces: Arc<dyn AiTraceStore>,
    ) -> Self {
        Self {
            mode,
            entitlement,
            quota,
            usage,
            prompts,
            tools,
            provider,
            traces,
        }
    }

    pub async fn invoke(
        &self,
        request: AiInvocationRequest,
    ) -> Result<AiInvocationResponse, AiInvocationError> {
        if self.mode == AiCapabilityMode::Disabled {
            self.append_trace(
                &request,
                None,
                None,
                AiTraceOutcome::Denied,
                Some("disabled"),
            )
            .await?;
            return Err(AiInvocationError::Disabled);
        }

        match self
            .entitlement
            .check(&request.tenant, &request.subject, &request.capability)
            .await?
        {
            EntitlementDecision::Allowed => {}
            EntitlementDecision::Denied { reason } => {
                self.append_trace(&request, None, None, AiTraceOutcome::Denied, Some(&reason))
                    .await?;
                return Err(AiInvocationError::EntitlementDenied(reason));
            }
        }

        for tool in &request.requested_tools {
            match self
                .tools
                .check_tool(&request.tenant, &request.subject, tool)
                .await?
            {
                ToolPermissionDecision::Allowed => {}
                ToolPermissionDecision::Denied { reason } => {
                    self.append_trace(&request, None, None, AiTraceOutcome::Denied, Some(&reason))
                        .await?;
                    return Err(AiInvocationError::ToolDenied(reason));
                }
            }
        }

        let prompt = self
            .prompts
            .select_prompt(&request.prompt_key, request.prompt_version.as_deref())
            .await?;

        let reservation = match self
            .quota
            .reserve(
                &request.tenant,
                &request.subject,
                &request.capability,
                request.max_tokens,
            )
            .await?
        {
            QuotaDecision::Reserved(reservation) => reservation,
            QuotaDecision::Denied { reason } => {
                self.append_trace(
                    &request,
                    Some(&prompt),
                    None,
                    AiTraceOutcome::Denied,
                    Some(&reason),
                )
                .await?;
                return Err(AiInvocationError::QuotaDenied(reason));
            }
        };

        let provider_request = LlmProviderRequest {
            invocation_id: request.invocation_id.clone(),
            model: request.model.clone(),
            system_prompt: prompt.template.clone(),
            user_input: request.user_input.clone(),
            tools: request.requested_tools.clone(),
            max_tokens: request.max_tokens,
        };

        let provider_response = match self.provider.complete(provider_request).await {
            Ok(response) => response,
            Err(error) => {
                self.quota.release(&reservation.reservation_id).await?;
                self.append_trace(
                    &request,
                    Some(&prompt),
                    None,
                    AiTraceOutcome::Failed,
                    Some(&error.to_string()),
                )
                .await?;
                return Err(AiInvocationError::Provider(error));
            }
        };

        self.usage
            .record(
                UsageEvent::new(
                    request.tenant.clone(),
                    request.subject.clone(),
                    "llm.tokens",
                    provider_response.total_tokens(),
                    "token",
                    "ai_invocation",
                    request.invocation_id.clone(),
                )
                .idempotency_key(
                    request
                        .idempotency_key
                        .clone()
                        .unwrap_or_else(|| request.invocation_id.clone()),
                )
                .metadata(serde_json::json!({
                    "model": request.model,
                    "prompt_key": prompt.prompt_key,
                    "prompt_version": prompt.version,
                    "provider_request_id": provider_response.provider_request_id,
                    "input_tokens": provider_response.input_tokens,
                    "output_tokens": provider_response.output_tokens,
                })),
            )
            .await?;
        self.quota.commit(&reservation.reservation_id).await?;
        self.append_trace(
            &request,
            Some(&prompt),
            Some(&provider_response),
            AiTraceOutcome::Succeeded,
            None,
        )
        .await?;

        Ok(AiInvocationResponse {
            invocation_id: request.invocation_id,
            prompt_version: prompt,
            provider_response,
        })
    }

    async fn append_trace(
        &self,
        request: &AiInvocationRequest,
        prompt: Option<&PromptVersion>,
        response: Option<&LlmProviderResponse>,
        outcome: AiTraceOutcome,
        error: Option<&str>,
    ) -> Result<(), AiTraceStoreError> {
        self.traces
            .append(AiTraceEvent {
                trace_id: uuid::Uuid::now_v7().to_string(),
                occurred_at: Utc::now(),
                invocation_id: request.invocation_id.clone(),
                tenant: request.tenant.clone(),
                subject: request.subject.clone(),
                capability: request.capability.clone(),
                prompt_key: request.prompt_key.clone(),
                prompt_version: prompt.map(|prompt| prompt.version.clone()),
                provider_request_id: response.map(|response| response.provider_request_id.clone()),
                model: request.model.clone(),
                input_tokens: response.map(|response| response.input_tokens).unwrap_or(0),
                output_tokens: response.map(|response| response.output_tokens).unwrap_or(0),
                outcome,
                error: error.map(str::to_string),
                metadata: redact_metadata(request.metadata.clone()),
            })
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use commercial::{InMemoryQuotaLedger, InMemoryUsageMeter, StaticEntitlementResolver};

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
