//! BFF authorization composition helpers.

use crate::audit::{AuditEvent, AuditOutcome};
use crate::error::{BffError, BffResult};
use crate::state::BffState;

/// Perform an authorization check against the configured authz adapter.
pub async fn check_authz(
    state: &BffState,
    user: &str,
    relation: &str,
    object: &str,
) -> BffResult<()> {
    let user_key = format!("user:{user}");
    let allowed = state
        .authz()
        .check(&user_key, relation, object)
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, "authz check failed");
            BffError::Internal("Authorization check failed".to_string())
        })?;

    state
        .append_audit(
            AuditEvent::new(
                format!("authz.{relation}"),
                object_type(object),
                object,
                if allowed {
                    AuditOutcome::Allowed
                } else {
                    AuditOutcome::Denied
                },
            )
            .actor(user.to_string()),
        )
        .await;

    allowed.then_some(()).ok_or_else(|| {
        tracing::warn!(
            user = user,
            relation = relation,
            object = object,
            "authz: permission denied"
        );
        BffError::Forbidden(format!(
            "Permission denied: user {user} cannot {relation} {object}"
        ))
    })
}

fn object_type(object: &str) -> String {
    object
        .split_once(':')
        .map(|(kind, _)| kind)
        .unwrap_or(object)
        .to_string()
}
