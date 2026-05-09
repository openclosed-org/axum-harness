//! BFF tenant context resolution for authenticated requests.

use user_service::ports::UserTenantRepository;

use crate::audit::{AuditEvent, AuditOutcome};
use crate::error::{BffError, BffResult};
use crate::request_context::RequestContext;
use crate::state::BffState;
pub use security_context::TenantContext;

pub async fn resolve_tenant_id(
    state: &BffState,
    request_context: &RequestContext,
) -> BffResult<kernel::TenantId> {
    resolve_tenant_context(state, request_context)
        .await
        .map(|context| context.tenant_id)
}

pub async fn resolve_tenant_context(
    state: &BffState,
    request_context: &RequestContext,
) -> BffResult<TenantContext> {
    let user_sub = request_context.user_sub();
    let binding_repo = state
        .user_tenant_repository()
        .ok_or_else(|| BffError::Internal("Database not initialized".to_string()))?;
    let tenant_id = binding_repo
        .find_user_tenant(user_sub)
        .await
        .map_err(map_tenant_resolution_error)?
        .map(|binding| binding.tenant_id);

    let resolved = if let Some(tenant_id) = tenant_id {
        kernel::TenantId(tenant_id)
    } else {
        state
            .append_audit(
                base_tenant_audit(request_context, AuditOutcome::Denied)
                    .metadata(serde_json::json!({"reason":"no_binding"})),
            )
            .await;
        return Err(BffError::Unauthorized(
            "No tenant binding found for authenticated user".to_string(),
        ));
    };

    if let Some(claim_tenant_id) = request_context.tenant_id()
        && claim_tenant_id != resolved.as_str()
    {
        tracing::warn!(
            user_sub = %request_context.user_sub(),
            claim_tenant_id,
            resolved_tenant_id = %resolved,
            "tenant claim does not match persisted tenant binding"
        );
        state
            .append_audit(
                base_tenant_audit(request_context, AuditOutcome::Denied)
                    .tenant(resolved.as_str())
                    .metadata(serde_json::json!({
                        "reason":"claim_mismatch",
                        "claim_tenant_id": claim_tenant_id,
                    })),
            )
            .await;
        return Err(BffError::Forbidden(
            "Tenant claim does not match authenticated user binding".to_string(),
        ));
    }

    let context = TenantContext {
        tenant_id: resolved,
        actor_sub: request_context.user_sub().to_string(),
        claim_tenant_id: request_context.tenant_id().map(str::to_string),
    };
    state
        .append_audit(
            base_tenant_audit(request_context, AuditOutcome::Allowed)
                .tenant(context.tenant_id.as_str())
                .metadata(serde_json::json!({"claim_tenant_id": context.claim_tenant_id})),
        )
        .await;
    Ok(context)
}

fn base_tenant_audit(request_context: &RequestContext, outcome: AuditOutcome) -> AuditEvent {
    AuditEvent::new("tenant.resolve", "tenant", "binding", outcome)
        .actor(request_context.user_sub().to_string())
        .request(request_context.request_id(), request_context.trace_id())
}

fn map_tenant_resolution_error(error: user_service::domain::error::UserError) -> BffError {
    let message = error.to_string();
    if message.contains("no such table: user_tenant") {
        return BffError::Unauthorized(
            "No tenant binding found for authenticated user".to_string(),
        );
    }

    BffError::Dependency(format!("Failed to resolve tenant binding: {message}"))
}
