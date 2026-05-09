//! SurrealDB implementation of CounterRepository.

use std::collections::BTreeMap;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use data::ports::surreal_db::{SurrealDbPort, SurrealFieldValue, TenantQueryOperation};
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::domain::{Counter, CounterId};
use crate::ports::{
    CommitOutcome, CounterMutation, CounterOperation, CounterRepository, RepositoryError,
};

pub struct SurrealDbCounterRepository<P: SurrealDbPort> {
    port: P,
}

impl<P: SurrealDbPort> SurrealDbCounterRepository<P> {
    pub fn new(port: P) -> Self {
        Self { port }
    }
}

#[derive(Debug, Deserialize)]
struct CounterRow {
    #[serde(rename = "counter_key")]
    id: String,
    value: i64,
    version: i64,
    updated_at: String,
}

#[derive(Debug, Deserialize)]
struct IdempotencyRow {
    request_hash: String,
    status: String,
    result_value: Option<i64>,
    result_version: Option<i64>,
}

#[async_trait]
impl<P: SurrealDbPort> CounterRepository for SurrealDbCounterRepository<P> {
    async fn load(&self, id: &CounterId) -> Result<Option<Counter>, RepositoryError> {
        let rows: Vec<CounterRow> = self.select_counter(id).await?;
        Ok(rows.into_iter().next().map(row_to_counter))
    }

    async fn increment(
        &self,
        id: &CounterId,
        expected_version: i64,
        now: DateTime<Utc>,
    ) -> Result<(i64, i64), RepositoryError> {
        self.simple_mutation(id, expected_version, CounterOperation::Increment, now)
            .await
    }

    async fn decrement(
        &self,
        id: &CounterId,
        expected_version: i64,
        now: DateTime<Utc>,
    ) -> Result<(i64, i64), RepositoryError> {
        self.simple_mutation(id, expected_version, CounterOperation::Decrement, now)
            .await
    }

    async fn reset(
        &self,
        id: &CounterId,
        expected_version: i64,
        now: DateTime<Utc>,
    ) -> Result<i64, RepositoryError> {
        let (_, version) = self
            .simple_mutation(id, expected_version, CounterOperation::Reset, now)
            .await?;
        Ok(version)
    }

    async fn upsert(&self, counter: &Counter) -> Result<(), RepositoryError> {
        if self.load(&counter.id).await?.is_some() {
            let mut set = BTreeMap::new();
            set.insert("value".to_string(), serde_json::json!(counter.value).into());
            set.insert(
                "version".to_string(),
                serde_json::json!(counter.version).into(),
            );
            set.insert(
                "updated_at".to_string(),
                serde_json::json!(counter.updated_at.to_rfc3339()).into(),
            );
            let _: Vec<CounterRow> = self
                .port
                .tenant_query(TenantQueryOperation::update(
                    "counter",
                    set,
                    key_filter(&counter.id),
                )?)
                .await?;
            return Ok(());
        }

        let mut values = BTreeMap::new();
        values.insert(
            "counter_key".to_string(),
            serde_json::json!(counter.id.as_str()).into(),
        );
        values.insert("value".to_string(), serde_json::json!(counter.value).into());
        values.insert(
            "version".to_string(),
            serde_json::json!(counter.version).into(),
        );
        values.insert("updated_at".to_string(), SurrealFieldValue::TimeNow);
        let _: Vec<CounterRow> = self
            .port
            .tenant_query(TenantQueryOperation::create("counter", values)?)
            .await?;
        Ok(())
    }

    async fn write_outbox(
        &self,
        event_id: &str,
        event_type: &str,
        payload: &str,
        source_service: &str,
        correlation_id: Option<&str>,
    ) -> Result<(), RepositoryError> {
        let mut values = BTreeMap::new();
        values.insert("event_id".to_string(), serde_json::json!(event_id).into());
        values.insert(
            "event_type".to_string(),
            serde_json::json!(event_type).into(),
        );
        values.insert(
            "event_payload".to_string(),
            serde_json::json!(payload).into(),
        );
        values.insert(
            "source_service".to_string(),
            serde_json::json!(source_service).into(),
        );
        values.insert(
            "correlation_id".to_string(),
            serde_json::json!(correlation_id).into(),
        );
        values.insert("status".to_string(), serde_json::json!("pending").into());
        let _: Vec<serde_json::Value> = self
            .port
            .tenant_query(TenantQueryOperation::create("event_outbox", values)?)
            .await?;
        Ok(())
    }

    async fn commit_mutation(
        &self,
        mutation: &CounterMutation<'_>,
        idempotency_key: Option<&str>,
    ) -> Result<CommitOutcome, RepositoryError> {
        let operation = mutation.operation.as_str();
        let request_hash = request_hash(mutation.counter_id.as_str(), "counter", operation);

        if let Some(key) = idempotency_key
            && let Some(outcome) = self
                .load_idempotency_outcome(mutation.counter_id, key, &request_hash)
                .await?
        {
            return Ok(outcome);
        }

        let current = self.load(mutation.counter_id).await?;
        let expected_version = mutation.new_version - 1;
        if current.as_ref().map(|counter| counter.version).unwrap_or(0) != expected_version {
            return Ok(CommitOutcome::CasConflict);
        }

        if current.is_some() {
            let mut set = BTreeMap::new();
            set.insert(
                "value".to_string(),
                serde_json::json!(mutation.new_value).into(),
            );
            set.insert(
                "version".to_string(),
                serde_json::json!(mutation.new_version).into(),
            );
            set.insert("updated_at".to_string(), SurrealFieldValue::TimeNow);
            let _: Vec<CounterRow> = self
                .port
                .tenant_query(TenantQueryOperation::update(
                    "counter",
                    set,
                    key_filter(mutation.counter_id),
                )?)
                .await?;
        } else {
            self.upsert(&Counter {
                id: mutation.counter_id.clone(),
                value: mutation.new_value,
                version: mutation.new_version,
                updated_at: Utc::now(),
            })
            .await?;
        }

        self.write_outbox(
            mutation.event_id,
            mutation.event_type,
            mutation.event_payload,
            mutation.source_service,
            mutation.correlation_id,
        )
        .await?;

        if let Some(key) = idempotency_key {
            self.write_idempotency_completion(mutation, key, &request_hash)
                .await?;
        }

        Ok(CommitOutcome::Committed {
            new_value: mutation.new_value,
            new_version: mutation.new_version,
        })
    }
}

impl<P: SurrealDbPort> SurrealDbCounterRepository<P> {
    async fn simple_mutation(
        &self,
        id: &CounterId,
        expected_version: i64,
        operation: CounterOperation,
        _now: DateTime<Utc>,
    ) -> Result<(i64, i64), RepositoryError> {
        let current = self.load(id).await?;
        if current.as_ref().map(|counter| counter.version).unwrap_or(0) != expected_version {
            return Err("CAS conflict".into());
        }
        let current_value = current.as_ref().map(|counter| counter.value).unwrap_or(0);
        let value = match operation {
            CounterOperation::Increment => current_value + 1,
            CounterOperation::Decrement => current_value - 1,
            CounterOperation::Reset => 0,
        };
        let version = expected_version + 1;
        self.upsert(&Counter {
            id: id.clone(),
            value,
            version,
            updated_at: Utc::now(),
        })
        .await?;
        Ok((value, version))
    }

    async fn select_counter(&self, id: &CounterId) -> Result<Vec<CounterRow>, RepositoryError> {
        self.port
            .tenant_query(TenantQueryOperation::select(
                "counter",
                Vec::new(),
                key_filter(id),
                None,
                Some(1),
            )?)
            .await
    }

    async fn load_idempotency_outcome(
        &self,
        counter_id: &CounterId,
        key: &str,
        request_hash: &str,
    ) -> Result<Option<CommitOutcome>, RepositoryError> {
        let mut filters = key_filter(counter_id);
        filters.insert("idempotency_key".to_string(), serde_json::json!(key).into());
        let rows: Vec<IdempotencyRow> = self
            .port
            .tenant_query(TenantQueryOperation::select(
                "counter_idempotency",
                Vec::new(),
                filters,
                None,
                Some(1),
            )?)
            .await?;

        let Some(row) = rows.first() else {
            return Ok(None);
        };
        if row.request_hash != request_hash {
            return Ok(Some(CommitOutcome::IdempotencyConflict));
        }
        if row.status == "completed"
            && let (Some(value), Some(version)) = (row.result_value, row.result_version)
        {
            return Ok(Some(CommitOutcome::IdempotentReplay { value, version }));
        }
        Ok(None)
    }

    async fn write_idempotency_completion(
        &self,
        mutation: &CounterMutation<'_>,
        key: &str,
        request_hash: &str,
    ) -> Result<(), RepositoryError> {
        let mut values = BTreeMap::new();
        values.insert(
            "counter_key".to_string(),
            serde_json::json!(mutation.counter_id.as_str()).into(),
        );
        values.insert("idempotency_key".to_string(), serde_json::json!(key).into());
        values.insert(
            "request_hash".to_string(),
            serde_json::json!(request_hash).into(),
        );
        values.insert(
            "operation".to_string(),
            serde_json::json!(mutation.operation.as_str()).into(),
        );
        values.insert("status".to_string(), serde_json::json!("completed").into());
        values.insert(
            "result_value".to_string(),
            serde_json::json!(mutation.new_value).into(),
        );
        values.insert(
            "result_version".to_string(),
            serde_json::json!(mutation.new_version).into(),
        );
        values.insert("completed_at".to_string(), SurrealFieldValue::TimeNow);
        let _: Vec<serde_json::Value> = self
            .port
            .tenant_query(TenantQueryOperation::create("counter_idempotency", values)?)
            .await?;
        Ok(())
    }
}

fn key_filter(id: &CounterId) -> BTreeMap<String, SurrealFieldValue> {
    BTreeMap::from([(
        "counter_key".to_string(),
        serde_json::json!(id.as_str()).into(),
    )])
}

fn row_to_counter(row: CounterRow) -> Counter {
    Counter {
        id: CounterId::new(row.id),
        value: row.value,
        version: row.version,
        updated_at: chrono::DateTime::parse_from_rfc3339(&row.updated_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
    }
}

fn request_hash(tenant_id: &str, resource: &str, operation: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"counter-service:v1:");
    hasher.update(tenant_id.as_bytes());
    hasher.update(b":");
    hasher.update(resource.as_bytes());
    hasher.update(b":");
    hasher.update(operation.as_bytes());
    hex::encode(hasher.finalize())
}
