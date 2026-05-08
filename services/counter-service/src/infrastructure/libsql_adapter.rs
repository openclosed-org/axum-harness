//! CounterRepository implementation backed by libsql (Turso embedded).
//!
//! This adapter translates the abstract CounterRepository trait into
//! concrete SQL operations. It handles:
//! - Counter upsert with CAS version check (optimistic locking)
//! - Atomic increment/decrement/reset with version field
//! - Unified event_outbox writes (no per-service private outbox)
//! - Timestamp management via datetime('now')

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use data::ports::lib_sql::LibSqlPort;
use event_bus::outbox::{OUTBOX_PENDING_INDEX_SQL, OUTBOX_TABLE_SQL};
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::domain::{Counter, CounterId};
use crate::ports::{
    CommitOutcome, CounterMutation, CounterOperation, CounterRepository, RepositoryError,
};

/// Raw row shape from the counter table.
#[derive(Debug, Deserialize)]
struct CounterRow {
    tenant_id: String,
    value: i64,
    version: i64,
    updated_at: String,
}

/// Minimal row shape for value-only queries from counter table.
#[derive(Debug, Deserialize)]
struct ValueRow {
    // Reused for CAS mutation results
    value: i64,
    version: i64,
}

/// Row shape from the counter_idempotency table.
#[derive(Debug, Deserialize)]
struct IdempotencyRow {
    request_hash: String,
    operation: String,
    status: String,
    result_value: Option<i64>,
    result_version: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct TableColumnRow {
    name: String,
}

/// CounterRepository backed by a libsql port.
///
/// This is the **primary** repository implementation used in Phase 0
/// where the monolith uses embedded Turso (libsql) for storage.
pub struct LibSqlCounterRepository<P: LibSqlPort> {
    pub port: P,
}

impl<P: LibSqlPort> LibSqlCounterRepository<P> {
    pub fn new(port: P) -> Self {
        Self { port }
    }

    /// Run the counter table migrations (idempotent).
    ///
    /// This should be called at application startup by the composition root.
    /// SQL schema is loaded from the migration file at `migrations/001_create_counter.sql`
    /// to maintain a single source of truth for the database schema.
    ///
    /// Note: `execute_batch` is currently used here only for startup migration
    /// batching. It is not yet treated as proof that the libSQL/Turso adapter is
    /// safe for higher-concurrency mutation paths under load; that still needs a
    /// dedicated validation pass against the real driver.
    pub async fn migrate(&self) -> Result<(), RepositoryError> {
        let migration_sql = include_str!("../../migrations/001_create_counter.sql");

        self.port.execute_batch(OUTBOX_TABLE_SQL).await?;
        self.port.execute_batch(OUTBOX_PENDING_INDEX_SQL).await?;
        self.port.execute_batch(migration_sql).await?;
        self.ensure_idempotency_columns().await?;
        Ok(())
    }

    async fn ensure_idempotency_columns(&self) -> Result<(), RepositoryError> {
        let rows: Vec<TableColumnRow> = self
            .port
            .query("PRAGMA table_info(counter_idempotency)", vec![])
            .await?;
        let has_column = |name: &str| rows.iter().any(|row| row.name == name);

        if !has_column("tenant_id") {
            self.port
                .execute(
                    "ALTER TABLE counter_idempotency ADD COLUMN tenant_id TEXT NOT NULL DEFAULT ''",
                    vec![],
                )
                .await?;
        }
        if !has_column("resource") {
            self.port
                .execute(
                    "ALTER TABLE counter_idempotency ADD COLUMN resource TEXT NOT NULL DEFAULT 'counter'",
                    vec![],
                )
                .await?;
        }
        if !has_column("response_digest") {
            self.port
                .execute(
                    "ALTER TABLE counter_idempotency ADD COLUMN response_digest TEXT",
                    vec![],
                )
                .await?;
        }

        Ok(())
    }
}

#[async_trait]
impl<P: LibSqlPort> CounterRepository for LibSqlCounterRepository<P> {
    async fn load(&self, id: &CounterId) -> Result<Option<Counter>, RepositoryError> {
        let rows: Vec<CounterRow> = self
            .port
            .query(
                "SELECT tenant_id, value, version, updated_at FROM counter WHERE tenant_id = ?",
                vec![id.as_str().to_string()],
            )
            .await?;

        let row = match rows.first() {
            Some(r) => r,
            None => return Ok(None),
        };

        let updated_at = DateTime::parse_from_rfc3339(&row.updated_at)
            .or_else(|_| DateTime::parse_from_str(&row.updated_at, "%Y-%m-%d %H:%M:%S"))
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now());

        Ok(Some(Counter {
            id: CounterId::new(&row.tenant_id),
            value: row.value,
            version: row.version,
            updated_at,
        }))
    }

    async fn increment(
        &self,
        id: &CounterId,
        expected_version: i64,
        _now: DateTime<Utc>,
    ) -> Result<(i64, i64), RepositoryError> {
        if expected_version == 0 {
            // New counter: insert with initial values
            let rows_affected = self
                .port
                .execute(
                    "INSERT INTO counter (tenant_id, value, version, updated_at) \
                     VALUES (?, 1, 1, datetime('now')) \
                     ON CONFLICT(tenant_id) DO UPDATE SET \
                         value = value + 1, version = version + 1, \
                         updated_at = datetime('now') \
                     WHERE version = ?",
                    vec![id.as_str().to_string(), expected_version.to_string()],
                )
                .await?;

            if rows_affected == 0 {
                // Conflict: counter exists but version mismatch
                return Err("CAS conflict on increment".into());
            }
        } else {
            // Existing counter: CAS update
            let rows_affected = self
                .port
                .execute(
                    "UPDATE counter SET value = value + 1, version = version + 1, \
                     updated_at = datetime('now') \
                     WHERE tenant_id = ? AND version = ?",
                    vec![id.as_str().to_string(), expected_version.to_string()],
                )
                .await?;

            if rows_affected == 0 {
                return Err("CAS conflict on increment".into());
            }
        }

        // Read back the new value and version
        let rows: Vec<ValueRow> = self
            .port
            .query(
                "SELECT value, version FROM counter WHERE tenant_id = ?",
                vec![id.as_str().to_string()],
            )
            .await?;

        let row = rows.first().ok_or("counter not found after increment")?;
        Ok((row.value, row.version))
    }

    async fn decrement(
        &self,
        id: &CounterId,
        expected_version: i64,
        _now: DateTime<Utc>,
    ) -> Result<(i64, i64), RepositoryError> {
        let rows_affected = self
            .port
            .execute(
                "UPDATE counter SET value = value - 1, version = version + 1, \
                 updated_at = datetime('now') \
                 WHERE tenant_id = ? AND version = ?",
                vec![id.as_str().to_string(), expected_version.to_string()],
            )
            .await?;

        if rows_affected == 0 {
            return Err("CAS conflict on decrement".into());
        }

        let rows: Vec<ValueRow> = self
            .port
            .query(
                "SELECT value, version FROM counter WHERE tenant_id = ?",
                vec![id.as_str().to_string()],
            )
            .await?;

        let row = rows.first().ok_or("counter not found after decrement")?;
        Ok((row.value, row.version))
    }

    async fn reset(
        &self,
        id: &CounterId,
        expected_version: i64,
        _now: DateTime<Utc>,
    ) -> Result<i64, RepositoryError> {
        let rows_affected = self
            .port
            .execute(
                "UPDATE counter SET value = 0, version = version + 1, \
                 updated_at = datetime('now') \
                 WHERE tenant_id = ? AND version = ?",
                vec![id.as_str().to_string(), expected_version.to_string()],
            )
            .await?;

        if rows_affected == 0 {
            return Err("CAS conflict on reset".into());
        }

        let rows: Vec<ValueRow> = self
            .port
            .query(
                "SELECT value, version FROM counter WHERE tenant_id = ?",
                vec![id.as_str().to_string()],
            )
            .await?;

        let row = rows.first().ok_or("counter not found after reset")?;
        Ok(row.version)
    }

    async fn upsert(&self, counter: &Counter) -> Result<(), RepositoryError> {
        self.port
            .execute(
                "INSERT INTO counter (tenant_id, value, version, updated_at) \
                 VALUES (?, ?, ?, ?) \
                 ON CONFLICT(tenant_id) DO UPDATE SET \
                     value = excluded.value, \
                     version = excluded.version, \
                     updated_at = excluded.updated_at",
                vec![
                    counter.id.as_str().to_string(),
                    counter.value.to_string(),
                    counter.version.to_string(),
                    counter.updated_at.format("%Y-%m-%d %H:%M:%S").to_string(),
                ],
            )
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
        self.port
            .execute(
                "INSERT INTO event_outbox (event_id, event_type, event_payload, source_service, correlation_id, status) \
                 VALUES (?, ?, ?, ?, ?, 'pending')",
                vec![
                    event_id.to_string(),
                    event_type.to_string(),
                    payload.to_string(),
                    source_service.to_string(),
                    correlation_id.unwrap_or_default().to_string(),
                ],
            )
            .await?;
        Ok(())
    }

    async fn commit_mutation(
        &self,
        m: &CounterMutation<'_>,
        idempotency_key: Option<&str>,
    ) -> Result<CommitOutcome, RepositoryError> {
        let expected_version = m.new_version - 1;
        let operation = m.operation.as_str();
        let resource = "counter";
        let request_hash = request_hash(m.counter_id.as_str(), resource, operation);

        if let Some(key) = idempotency_key
            && let Some(outcome) = self
                .load_idempotency_outcome(m.counter_id.as_str(), key, &request_hash)
                .await?
        {
            return Ok(outcome);
        }

        // ── Begin typed transaction (compile-time connection guarantee) ──
        let tx = self.port.begin().await?;

        if let Some(key) = idempotency_key {
            let rows = tx
                .execute(
                    "INSERT INTO counter_idempotency \
                     (counter_id, idempotency_key, request_hash, tenant_id, resource, operation, status) \
                     VALUES (?, ?, ?, ?, ?, ?, 'in_progress') \
                     ON CONFLICT(counter_id, idempotency_key) DO NOTHING",
                    vec![
                        m.counter_id.as_str().to_string(),
                        key.to_string(),
                        request_hash.clone(),
                        m.counter_id.as_str().to_string(),
                        resource.to_string(),
                        operation.to_string(),
                    ],
                )
                .await?;

            if rows == 0 {
                tx.rollback().await?;
                return self
                    .load_idempotency_outcome(m.counter_id.as_str(), key, &request_hash)
                    .await?
                    .ok_or_else(|| "idempotency key is still in progress".into());
            }
        }

        // ── CAS mutation ──
        let rows_affected = if expected_version == 0 {
            let insert_value = match m.operation {
                CounterOperation::Increment => 1,
                CounterOperation::Decrement => -1,
                CounterOperation::Reset => 0,
            };
            tx.execute(
                "INSERT INTO counter (tenant_id, value, version, updated_at) \
                 VALUES (?, ?, 1, datetime('now')) \
                 ON CONFLICT(tenant_id) DO NOTHING",
                vec![m.counter_id.as_str().to_string(), insert_value.to_string()],
            )
            .await?
        } else {
            let set_clause = match m.operation {
                CounterOperation::Increment => "value = value + 1",
                CounterOperation::Decrement => "value = value - 1",
                CounterOperation::Reset => "value = 0",
            };
            let sql = format!(
                "UPDATE counter SET {set_clause}, version = version + 1, \
                 updated_at = datetime('now') \
                 WHERE tenant_id = ? AND version = ?"
            );
            tx.execute(
                &sql,
                vec![
                    m.counter_id.as_str().to_string(),
                    expected_version.to_string(),
                ],
            )
            .await?
        };

        if rows_affected == 0 {
            // CAS conflict: version mismatch. Rollback outbox entry too.
            tx.rollback().await?;
            return Ok(CommitOutcome::CasConflict);
        }

        // ── Outbox write (same transaction — either both or neither) ──
        tx.execute(
            "INSERT INTO event_outbox \
             (event_id, event_type, event_payload, source_service, correlation_id, status) \
             VALUES (?, ?, ?, ?, ?, 'pending')",
            vec![
                m.event_id.to_string(),
                m.event_type.to_string(),
                m.event_payload.to_string(),
                m.source_service.to_string(),
                m.correlation_id.unwrap_or_default().to_string(),
            ],
        )
        .await?;

        if let Some(key) = idempotency_key {
            let response_digest = response_digest(m.new_value, m.new_version);
            tx.execute(
                "UPDATE counter_idempotency \
                 SET status = 'completed', result_value = ?, result_version = ?, response_digest = ?, completed_at = datetime('now') \
                 WHERE counter_id = ? AND idempotency_key = ? AND request_hash = ?",
                vec![
                    m.new_value.to_string(),
                    m.new_version.to_string(),
                    response_digest,
                    m.counter_id.as_str().to_string(),
                    key.to_string(),
                    request_hash,
                ],
            )
            .await?;
        }

        // ── Commit ──
        tx.commit().await?;

        Ok(CommitOutcome::Committed {
            new_value: m.new_value,
            new_version: m.new_version,
        })
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

fn response_digest(value: i64, version: i64) -> String {
    let mut hasher = Sha256::new();
    hasher.update(format!("value={value};version={version}").as_bytes());
    hex::encode(hasher.finalize())
}

impl<P: LibSqlPort> LibSqlCounterRepository<P> {
    async fn load_idempotency_outcome(
        &self,
        counter_id: &str,
        key: &str,
        request_hash: &str,
    ) -> Result<Option<CommitOutcome>, RepositoryError> {
        let rows: Vec<IdempotencyRow> = self
            .port
            .query(
                "SELECT request_hash, operation, status, result_value, result_version \
                 FROM counter_idempotency WHERE counter_id = ? AND idempotency_key = ?",
                vec![counter_id.to_string(), key.to_string()],
            )
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
}
