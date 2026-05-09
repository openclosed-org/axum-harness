//! BFF request context extracted at the HTTP boundary.

use authn_oidc_verifier::VerifiedIdentity;
use axum::extract::Request;
use counter_service::contracts::service::CounterCommandContext;
use observability::current_trace_context;
use security_context::{ExecutionContext, RequestContextIds, SecurityContext};

/// Request context extracted at the server boundary and forwarded into service calls.
#[derive(Debug, Clone)]
pub struct RequestContext {
    pub execution: ExecutionContext,
}

impl RequestContext {
    pub fn from_verified_identity(identity: VerifiedIdentity, request_id: Option<String>) -> Self {
        let trace_context = current_trace_context();
        let request = RequestContextIds {
            request_id,
            trace_id: trace_context
                .as_ref()
                .map(|context| context.trace_id.clone()),
            span_id: trace_context.map(|context| context.span_id),
        };
        Self {
            execution: ExecutionContext::new(
                SecurityContext::user(identity.sub, identity.tenant_id, identity.roles),
                request,
            ),
        }
    }

    pub fn from_dev_headers(req: &Request) -> Option<Self> {
        let user_sub = req
            .headers()
            .get("x-dev-user-sub")
            .and_then(|value| value.to_str().ok())?
            .trim()
            .to_string();
        if user_sub.is_empty() {
            return None;
        }

        let tenant_id = req
            .headers()
            .get("x-dev-tenant-id")
            .and_then(|value| value.to_str().ok())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned);
        let roles = req
            .headers()
            .get("x-dev-user-roles")
            .and_then(|value| value.to_str().ok())
            .map(parse_dev_roles)
            .unwrap_or_default();
        let request_id = request_id(req);
        let trace_context = current_trace_context();
        let request = RequestContextIds {
            request_id,
            trace_id: trace_context
                .as_ref()
                .map(|context| context.trace_id.clone()),
            span_id: trace_context.map(|context| context.span_id),
        };

        Some(Self {
            execution: ExecutionContext::new(
                SecurityContext::user(user_sub, tenant_id, roles),
                request,
            ),
        })
    }

    pub fn user_sub(&self) -> &str {
        &self.execution.security.subject
    }

    pub fn tenant_id(&self) -> Option<&str> {
        self.execution.security.tenant_id.as_deref()
    }

    pub fn request_id(&self) -> Option<String> {
        self.execution.request.request_id.clone()
    }

    pub fn trace_id(&self) -> Option<String> {
        self.execution.request.trace_id.clone()
    }

    pub fn to_counter_command_context(&self) -> CounterCommandContext {
        CounterCommandContext {
            correlation_id: self.execution.request.request_id.clone(),
            causation_id: self.execution.request.request_id.clone(),
            actor: Some(self.execution.security.actor.clone()),
            trace_id: self.execution.request.trace_id.clone(),
            span_id: self.execution.request.span_id.clone(),
        }
    }
}

pub fn request_id(req: &Request) -> Option<String> {
    req.headers()
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
}

fn parse_dev_roles(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_owned)
        .collect()
}
