//! BFF authorization composition helpers.

use crate::audit::{AuditEvent, AuditOutcome};
use crate::error::{BffError, BffResult};
use crate::request_context::RequestContext;
use crate::state::BffState;
use authz::{AuthzCheck, AuthzDecision};

/// Perform an authorization check against the configured authz adapter.
pub async fn check_authz(
    state: &BffState,
    request_context: &RequestContext,
    relation: &str,
    object: &str,
    tenant_id: Option<&str>,
) -> BffResult<()> {
    let user = request_context.user_sub();
    let user_key = format!("user:{user}");
    let check = AuthzCheck::new(user_key, relation, object)
        .request(request_context.request_id())
        .optional_tenant(tenant_id.map(str::to_string));
    let allowed = state
        .authz()
        .check(&check.user, &check.relation, &check.object)
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, "authz check failed");
            BffError::Internal("Authorization check failed".to_string())
        })?;
    let decision = AuthzDecision::from_allowed(allowed);
    let mut audit_event = AuditEvent::new(
        format!("authz.{}", check.relation),
        object_type(&check.object),
        &check.object,
        audit_outcome(decision),
    )
    .actor(user.to_string())
    .request(request_context.request_id(), request_context.trace_id())
    .metadata(serde_json::json!({
        "tenant_id": check.tenant_id.as_deref(),
        "request_id": check.request_id.as_deref(),
    }));
    if let Some(tenant_id) = check.tenant_id.as_deref() {
        audit_event = audit_event.tenant(tenant_id);
    }

    state.append_audit(audit_event).await;

    decision.is_allowed().then_some(()).ok_or_else(|| {
        tracing::warn!(
            user = user,
            relation = check.relation,
            object = check.object,
            "authz: permission denied"
        );
        BffError::Forbidden(format!(
            "Permission denied: user {user} cannot {} {}",
            check.relation, check.object
        ))
    })
}

fn audit_outcome(decision: AuthzDecision) -> AuditOutcome {
    match decision {
        AuthzDecision::Allowed => AuditOutcome::Allowed,
        AuthzDecision::Denied => AuditOutcome::Denied,
    }
}

fn object_type(object: &str) -> String {
    object
        .split_once(':')
        .map(|(kind, _)| kind)
        .unwrap_or(object)
        .to_string()
}
