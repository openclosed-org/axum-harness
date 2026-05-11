//! Counter BFF use cases.
//!
//! These functions are BFF composition logic: resolve authenticated tenant,
//! enforce route-level authz, call counter-service, and maintain BFF-local cache.

use counter_service::contracts::service::CounterError;
use counter_service::domain::CounterId;

use crate::audit::{AuditEvent, AuditOutcome};
use crate::authz::check_authz;
use crate::composition::CounterServiceHandle;
use crate::error::{BffError, BffResult};
use crate::request_context::RequestContext;
use crate::state::BffState;
use crate::tenant_context::resolve_tenant_id;

pub async fn increment(
    state: &BffState,
    request_context: &RequestContext,
    idempotency_key: Option<&str>,
) -> BffResult<i64> {
    mutate_counter(
        state,
        request_context,
        idempotency_key,
        CounterMutation::Increment,
    )
    .await
}

pub async fn decrement(
    state: &BffState,
    request_context: &RequestContext,
    idempotency_key: Option<&str>,
) -> BffResult<i64> {
    mutate_counter(
        state,
        request_context,
        idempotency_key,
        CounterMutation::Decrement,
    )
    .await
}

pub async fn reset(
    state: &BffState,
    request_context: &RequestContext,
    idempotency_key: Option<&str>,
) -> BffResult<i64> {
    mutate_counter(
        state,
        request_context,
        idempotency_key,
        CounterMutation::Reset,
    )
    .await
}

pub async fn get_value(state: &BffState, request_context: &RequestContext) -> BffResult<i64> {
    let tenant_id = resolve_tenant_id(state, request_context).await?;
    let cache_key = cache_key(tenant_id.as_str());

    check_authz(
        state,
        request_context,
        "can_read",
        &counter_object(tenant_id.as_str()),
        Some(tenant_id.as_str()),
    )
    .await?;

    if let Some(cached) = state.counter_cache().get(&cache_key).await {
        return Ok(cached);
    }

    let value = build_service(state)?
        .get_value(&CounterId::new(tenant_id.as_str()))
        .await
        .map_err(map_counter_error)?;

    state.counter_cache().insert(cache_key, value).await;
    Ok(value)
}

enum CounterMutation {
    Increment,
    Decrement,
    Reset,
}

async fn mutate_counter(
    state: &BffState,
    request_context: &RequestContext,
    idempotency_key: Option<&str>,
    mutation: CounterMutation,
) -> BffResult<i64> {
    let tenant_id = resolve_tenant_id(state, request_context).await?;
    check_authz(
        state,
        request_context,
        "can_write",
        &counter_object(tenant_id.as_str()),
        Some(tenant_id.as_str()),
    )
    .await?;

    let commercial_guard = state
        .commercial()
        .guard_counter_write(tenant_id.as_str(), request_context.user_sub())
        .await?;

    let service = build_service(state)?;
    let command_context = request_context.to_counter_command_context();
    let counter_id = CounterId::new(tenant_id.as_str());
    let value = match mutation {
        CounterMutation::Increment => {
            service
                .increment_with_context(&counter_id, idempotency_key, &command_context)
                .await
        }
        CounterMutation::Decrement => {
            service
                .decrement_with_context(&counter_id, idempotency_key, &command_context)
                .await
        }
        CounterMutation::Reset => {
            service
                .reset_with_context(&counter_id, idempotency_key, &command_context)
                .await
        }
    };

    let value = match value {
        Ok(value) => value,
        Err(error) => {
            commercial_guard.release().await;
            return Err(map_counter_error(error));
        }
    };

    commercial_guard.commit().await?;

    state
        .append_audit(
            AuditEvent::new(
                mutation.audit_action(),
                "counter",
                tenant_id.as_str(),
                AuditOutcome::Succeeded,
            )
            .actor(request_context.user_sub().to_string())
            .tenant(tenant_id.as_str())
            .request(request_context.request_id(), request_context.trace_id())
            .metadata(serde_json::json!({"idempotency_key":"[redacted]"})),
        )
        .await;

    state
        .counter_cache()
        .invalidate(&cache_key(tenant_id.as_str()))
        .await;
    Ok(value)
}

impl CounterMutation {
    fn audit_action(&self) -> &'static str {
        match self {
            Self::Increment => "counter.increment",
            Self::Decrement => "counter.decrement",
            Self::Reset => "counter.reset",
        }
    }
}

fn build_service(state: &BffState) -> BffResult<CounterServiceHandle> {
    state
        .counter_service()
        .ok_or_else(|| BffError::Internal("Embedded database not initialized".to_string()))
}

fn counter_object(tenant_id: &str) -> String {
    format!("counter:{tenant_id}")
}

fn cache_key(tenant_id: &str) -> String {
    format!("counter:{tenant_id}")
}

fn map_counter_error(err: CounterError) -> BffError {
    match err {
        CounterError::CasConflict | CounterError::CasConflictWithDetails { .. } => {
            BffError::Conflict("Counter was modified concurrently".to_string())
        }
        CounterError::NotFound(msg) => BffError::NotFound(msg),
        CounterError::IdempotencyConflict => BffError::Conflict(
            "Idempotency key was reused for a different counter command".to_string(),
        ),
        CounterError::Database(e) => {
            tracing::error!(error = %e, "counter database error");
            BffError::Dependency("Counter operation failed".to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counter_object_uses_authz_counter_namespace() {
        assert_eq!(counter_object("tenant-a"), "counter:tenant-a");
    }

    #[test]
    fn cache_key_tracks_counter_tenant_scope() {
        assert_eq!(cache_key("tenant-a"), "counter:tenant-a");
    }
}
