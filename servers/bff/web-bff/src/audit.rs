use async_trait::async_trait;
use data::ports::lib_sql::LibSqlPort;
use serde::Deserialize;
use std::sync::Arc;

use security_audit::InMemoryAuditSink;
pub use security_audit::{AuditError, AuditEvent, AuditOutcome, AuditSink, redact_metadata};

pub const AUDIT_EVENTS_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS audit_events (
    event_id TEXT PRIMARY KEY,
    occurred_at TEXT NOT NULL,
    actor_sub TEXT,
    tenant_id TEXT,
    action TEXT NOT NULL,
    resource_type TEXT NOT NULL,
    resource_id TEXT NOT NULL,
    outcome TEXT NOT NULL,
    request_id TEXT,
    trace_id TEXT,
    metadata TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_audit_events_tenant_occurred
    ON audit_events(tenant_id, occurred_at);
"#;

#[derive(Clone)]
pub struct BffAuditSink {
    memory: Arc<InMemoryAuditSink>,
    durable: Option<Arc<dyn DurableAuditWriter>>,
}

impl BffAuditSink {
    pub fn in_memory() -> Arc<Self> {
        Arc::new(Self {
            memory: InMemoryAuditSink::shared(),
            durable: None,
        })
    }

    pub fn durable<P>(port: P) -> Arc<Self>
    where
        P: LibSqlPort + Clone + Send + Sync + 'static,
    {
        Arc::new(Self {
            memory: InMemoryAuditSink::shared(),
            durable: Some(Arc::new(LibSqlAuditWriter { port })),
        })
    }

    pub async fn events(&self) -> Vec<AuditEvent> {
        if let Some(durable) = &self.durable {
            match durable.events().await {
                Ok(events) => return events,
                Err(error) => {
                    tracing::warn!(error = %error, "failed to read durable audit events");
                }
            }
        }

        self.memory.events().await
    }
}

#[async_trait]
impl AuditSink for BffAuditSink {
    async fn append(&self, event: AuditEvent) -> Result<(), AuditError> {
        self.memory.append(event.clone()).await?;
        if let Some(durable) = &self.durable {
            durable.append(event).await?;
        }
        Ok(())
    }
}

#[async_trait]
trait DurableAuditWriter: Send + Sync {
    async fn append(&self, event: AuditEvent) -> Result<(), AuditError>;
    async fn events(&self) -> Result<Vec<AuditEvent>, AuditError>;
}

struct LibSqlAuditWriter<P> {
    port: P,
}

#[derive(Debug, Deserialize)]
struct AuditEventRow {
    event_id: String,
    occurred_at: String,
    actor_sub: Option<String>,
    tenant_id: Option<String>,
    action: String,
    resource_type: String,
    resource_id: String,
    outcome: String,
    request_id: Option<String>,
    trace_id: Option<String>,
    metadata: String,
}

#[async_trait]
impl<P> DurableAuditWriter for LibSqlAuditWriter<P>
where
    P: LibSqlPort + Send + Sync,
{
    async fn append(&self, event: AuditEvent) -> Result<(), AuditError> {
        self.port
            .execute(
                "INSERT INTO audit_events (event_id, occurred_at, actor_sub, tenant_id, action, resource_type, resource_id, outcome, request_id, trace_id, metadata) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                vec![
                    event.event_id,
                    event.occurred_at.to_rfc3339(),
                    event.actor_sub.unwrap_or_default(),
                    event.tenant_id.unwrap_or_default(),
                    event.action,
                    event.resource_type,
                    event.resource_id,
                    audit_outcome_to_str(&event.outcome).to_string(),
                    event.request_id.unwrap_or_default(),
                    event.trace_id.unwrap_or_default(),
                    serde_json::to_string(&event.metadata)
                        .map_err(|error| AuditError::Sink(error.to_string()))?,
                ],
            )
            .await
            .map_err(|error| AuditError::Sink(error.to_string()))?;
        Ok(())
    }

    async fn events(&self) -> Result<Vec<AuditEvent>, AuditError> {
        let rows = self
            .port
            .query::<AuditEventRow>(
                "SELECT event_id, occurred_at, NULLIF(actor_sub, '') AS actor_sub, NULLIF(tenant_id, '') AS tenant_id, action, resource_type, resource_id, outcome, NULLIF(request_id, '') AS request_id, NULLIF(trace_id, '') AS trace_id, metadata FROM audit_events ORDER BY occurred_at ASC, event_id ASC",
                vec![],
            )
            .await
            .map_err(|error| AuditError::Sink(error.to_string()))?;

        rows.into_iter().map(row_to_event).collect()
    }
}

fn row_to_event(row: AuditEventRow) -> Result<AuditEvent, AuditError> {
    Ok(AuditEvent {
        event_id: row.event_id,
        occurred_at: chrono::DateTime::parse_from_rfc3339(&row.occurred_at)
            .map_err(|error| AuditError::Sink(error.to_string()))?
            .with_timezone(&chrono::Utc),
        actor_sub: row.actor_sub,
        tenant_id: row.tenant_id,
        action: row.action,
        resource_type: row.resource_type,
        resource_id: row.resource_id,
        outcome: parse_audit_outcome(&row.outcome)?,
        request_id: row.request_id,
        trace_id: row.trace_id,
        metadata: serde_json::from_str(&row.metadata)
            .map_err(|error| AuditError::Sink(error.to_string()))?,
    })
}

fn audit_outcome_to_str(outcome: &AuditOutcome) -> &'static str {
    match outcome {
        AuditOutcome::Allowed => "allowed",
        AuditOutcome::Denied => "denied",
        AuditOutcome::Succeeded => "succeeded",
        AuditOutcome::Failed => "failed",
    }
}

fn parse_audit_outcome(value: &str) -> Result<AuditOutcome, AuditError> {
    match value {
        "allowed" => Ok(AuditOutcome::Allowed),
        "denied" => Ok(AuditOutcome::Denied),
        "succeeded" => Ok(AuditOutcome::Succeeded),
        "failed" => Ok(AuditOutcome::Failed),
        other => Err(AuditError::Sink(format!("unknown audit outcome '{other}'"))),
    }
}
