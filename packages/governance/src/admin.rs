use async_trait::async_trait;
use security_audit::{AuditEvent, AuditOutcome, AuditSink};
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
