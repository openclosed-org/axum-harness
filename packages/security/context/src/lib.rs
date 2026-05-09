//! Shared security context contracts.
//!
//! This crate contains framework-neutral identity, tenant, and request context
//! shapes. HTTP extraction and service-specific conversions stay at the server
//! boundary.

use contracts_events::ActorRef;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecurityContext {
    pub subject: String,
    pub tenant_id: Option<String>,
    pub roles: Vec<String>,
    pub actor: ActorRef,
}

impl SecurityContext {
    pub fn user(subject: impl Into<String>, tenant_id: Option<String>, roles: Vec<String>) -> Self {
        let subject = subject.into();
        Self {
            subject: subject.clone(),
            tenant_id,
            roles,
            actor: ActorRef {
                actor_id: subject.clone(),
                actor_type: "user".to_string(),
                subject: Some(subject),
            },
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RequestContextIds {
    pub request_id: Option<String>,
    pub trace_id: Option<String>,
    pub span_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionContext {
    pub security: SecurityContext,
    pub request: RequestContextIds,
}

impl ExecutionContext {
    pub fn new(security: SecurityContext, request: RequestContextIds) -> Self {
        Self { security, request }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TenantContext {
    pub tenant_id: kernel::TenantId,
    pub actor_sub: String,
    pub claim_tenant_id: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_security_context_builds_stable_actor_ref() {
        let context = SecurityContext::user(
            "user-1",
            Some("tenant-1".to_string()),
            vec!["owner".to_string()],
        );

        assert_eq!(context.subject, "user-1");
        assert_eq!(context.actor.actor_type, "user");
        assert_eq!(context.actor.subject.as_deref(), Some("user-1"));
    }
}
