use crate::{
    AiCapabilityMode, AiTraceEvent, AiTraceOutcome, AiTraceStore, AiTraceStoreError,
    LlmProviderError, LlmProviderPort, LlmProviderRequest, LlmProviderResponse, PromptRegistry,
    PromptRegistryError, PromptVersion, ToolKey, ToolPermissionDecision, ToolPermissionError,
    ToolPermissionPolicy,
};
use chrono::Utc;
use commercial::{
    CapabilityKey, CommercialSubject, CommercialTenant, EntitlementDecision, EntitlementError,
    EntitlementResolver, QuotaDecision, QuotaLedger, QuotaLedgerError, UsageEvent, UsageMeter,
    UsageMeterError,
};
use security_audit::redact_metadata;
use std::sync::Arc;

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
pub struct AiInvocationResponse {
    pub invocation_id: String,
    pub prompt_version: PromptVersion,
    pub provider_response: LlmProviderResponse,
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
