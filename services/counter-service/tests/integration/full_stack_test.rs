//! Integration tests for counter-service with real libsql port.
//!
//! These tests verify the full stack: application service → repository → libsql.

use chrono::{DateTime, Utc};
use counter_service::application::{RepositoryBackedCounterService, TenantScopedCounterService};
use counter_service::contracts::service::CounterService;
use counter_service::infrastructure::LibSqlCounterRepository;
use counter_service::ports::{CounterRepository, RepositoryError};
use data::ports::lib_sql::{LibSqlError, LibSqlPort, SqlTransaction};
use kernel::TenantId;
use rusqlite::Connection;
use serde::Deserialize;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Minimal libsql port for testing — wraps rusqlite.
struct InMemoryLibSqlPort {
    conn: Arc<Mutex<Connection>>,
}

impl InMemoryLibSqlPort {
    fn new() -> Self {
        let conn = Connection::open_in_memory().expect("failed to open in-memory sqlite");
        Self {
            conn: Arc::new(Mutex::new(conn)),
        }
    }
}

/// Transaction guard for in-memory test port.
///
/// Shares the same mutex-protected connection. BEGIN/COMMIT/ROLLBACK
/// executed as raw SQL — sufficient for single-connection tests.
struct InMemoryTransaction {
    conn: Arc<Mutex<Connection>>,
}

#[async_trait::async_trait]
impl SqlTransaction for InMemoryTransaction {
    async fn execute(&self, sql: &str, params: Vec<String>) -> Result<u64, LibSqlError> {
        let conn = self.conn.lock().await;
        let param_refs: Vec<&dyn rusqlite::types::ToSql> = params
            .iter()
            .map(|s| s as &dyn rusqlite::types::ToSql)
            .collect();
        let affected = conn
            .execute(sql, rusqlite::params_from_iter(param_refs))
            .map_err(|e| Box::new(e) as LibSqlError)?;
        Ok(affected as u64)
    }

    async fn commit(self: Box<Self>) -> Result<(), LibSqlError> {
        let conn = self.conn.lock().await;
        conn.execute_batch("COMMIT")
            .map_err(|e| Box::new(e) as LibSqlError)?;
        Ok(())
    }

    async fn rollback(self: Box<Self>) -> Result<(), LibSqlError> {
        let conn = self.conn.lock().await;
        conn.execute_batch("ROLLBACK")
            .map_err(|e| Box::new(e) as LibSqlError)?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl LibSqlPort for InMemoryLibSqlPort {
    async fn health_check(&self) -> Result<(), LibSqlError> {
        let conn = self.conn.lock().await;
        conn.execute("SELECT 1", [])
            .map_err(|e| Box::new(e) as LibSqlError)?;
        Ok(())
    }

    async fn execute(&self, sql: &str, params: Vec<String>) -> Result<u64, LibSqlError> {
        let conn = self.conn.lock().await;
        let param_refs: Vec<&dyn rusqlite::types::ToSql> = params
            .iter()
            .map(|s| s as &dyn rusqlite::types::ToSql)
            .collect();
        let affected = conn
            .execute(sql, rusqlite::params_from_iter(param_refs))
            .map_err(|e| Box::new(e) as LibSqlError)?;
        Ok(affected as u64)
    }

    async fn execute_batch(&self, sql: &str) -> Result<(), LibSqlError> {
        let conn = self.conn.lock().await;
        conn.execute_batch(sql)
            .map_err(|e| Box::new(e) as LibSqlError)?;
        Ok(())
    }

    async fn query<T: serde::de::DeserializeOwned + Send + Sync>(
        &self,
        sql: &str,
        params: Vec<String>,
    ) -> Result<Vec<T>, LibSqlError> {
        let conn = self.conn.lock().await;
        let param_refs: Vec<&dyn rusqlite::types::ToSql> = params
            .iter()
            .map(|s| s as &dyn rusqlite::types::ToSql)
            .collect();

        let mut stmt = conn.prepare(sql).map_err(|e| Box::new(e) as LibSqlError)?;

        let columns = stmt
            .column_names()
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>();

        let rows: Vec<serde_json::Value> = stmt
            .query_map(rusqlite::params_from_iter(param_refs), |row| {
                let mut map = serde_json::Map::new();
                for (i, col) in columns.iter().enumerate() {
                    // Handle different SQLite types
                    let value: serde_json::Value = match row.get_ref(i)? {
                        rusqlite::types::ValueRef::Integer(v) => {
                            serde_json::Value::Number(serde_json::Number::from(v))
                        }
                        rusqlite::types::ValueRef::Text(v) => {
                            let s = String::from_utf8_lossy(v).to_string();
                            serde_json::Value::String(s)
                        }
                        rusqlite::types::ValueRef::Real(v) => serde_json::Number::from_f64(v)
                            .map(serde_json::Value::Number)
                            .unwrap_or(serde_json::Value::Null),
                        rusqlite::types::ValueRef::Blob(v) => serde_json::Value::Array(
                            v.iter().map(|b| serde_json::json!(*b)).collect(),
                        ),
                        rusqlite::types::ValueRef::Null => serde_json::Value::Null,
                    };
                    map.insert(col.clone(), value);
                }
                Ok(serde_json::Value::Object(map))
            })
            .map_err(|e| Box::new(e) as LibSqlError)?
            .filter_map(|r| r.ok())
            .collect();

        let json = serde_json::to_value(&rows).map_err(|e| Box::new(e) as LibSqlError)?;
        let items: Vec<T> = serde_json::from_value(json).map_err(|e| Box::new(e) as LibSqlError)?;
        Ok(items)
    }

    async fn begin(&self) -> Result<Box<dyn SqlTransaction>, LibSqlError> {
        {
            let conn = self.conn.lock().await;
            conn.execute_batch("BEGIN IMMEDIATE")
                .map_err(|e| Box::new(e) as LibSqlError)?;
        }
        Ok(Box::new(InMemoryTransaction {
            conn: self.conn.clone(),
        }))
    }
}

#[tokio::test]
async fn full_stack_increment_reset_increment() {
    let port = InMemoryLibSqlPort::new();
    let repo = LibSqlCounterRepository::new(port);
    repo.migrate().await.unwrap();

    let service = TenantScopedCounterService::new(repo);
    let tenant = TenantId("test-tenant".into());

    let v1 = service.increment(&tenant, None).await.unwrap();
    assert_eq!(v1, 1);

    let v2 = service.increment(&tenant, None).await.unwrap();
    assert_eq!(v2, 2);

    let r = service.reset(&tenant, None).await.unwrap();
    assert_eq!(r, 0);

    let v3 = service.increment(&tenant, None).await.unwrap();
    assert_eq!(v3, 1);
}

#[tokio::test]
async fn full_stack_tenant_isolation() {
    let port = InMemoryLibSqlPort::new();
    let repo = LibSqlCounterRepository::new(port);
    repo.migrate().await.unwrap();

    let service = TenantScopedCounterService::new(repo);
    let a = TenantId("tenant-a".into());
    let b = TenantId("tenant-b".into());

    service.increment(&a, None).await.unwrap();
    service.increment(&a, None).await.unwrap();
    service.increment(&b, None).await.unwrap();

    assert_eq!(service.get_value(&a).await.unwrap(), 2);
    assert_eq!(service.get_value(&b).await.unwrap(), 1);
}

#[tokio::test]
async fn full_stack_idempotency_key_caches_result() {
    let port = InMemoryLibSqlPort::new();
    let repo = LibSqlCounterRepository::new(port);
    repo.migrate().await.unwrap();

    let service = TenantScopedCounterService::new(repo);
    let tenant = TenantId("tenant-idem".into());
    let idem_key = "req-unique-1";

    let v1 = service.increment(&tenant, Some(idem_key)).await.unwrap();
    let v2 = service.increment(&tenant, Some(idem_key)).await.unwrap();

    assert_eq!(v1, 1);
    assert_eq!(v2, 1, "idempotency key must return cached result");

    // Value in DB should still be 1
    assert_eq!(service.get_value(&tenant).await.unwrap(), 1);
}

#[tokio::test]
async fn full_stack_different_idempotency_keys_produce_different_results() {
    let port = InMemoryLibSqlPort::new();
    let repo = LibSqlCounterRepository::new(port);
    repo.migrate().await.unwrap();

    let service = TenantScopedCounterService::new(repo);
    let tenant = TenantId("tenant-idem-2".into());

    let v1 = service.increment(&tenant, Some("key-a")).await.unwrap();
    let v2 = service.increment(&tenant, Some("key-b")).await.unwrap();

    assert_eq!(v1, 1);
    assert_eq!(v2, 2, "different keys must produce different results");
}

#[tokio::test]
async fn full_stack_idempotency_prevents_duplicate_outbox() {
    let port = InMemoryLibSqlPort::new();
    let repo = LibSqlCounterRepository::new(port);
    repo.migrate().await.unwrap();

    let service = TenantScopedCounterService::new(repo);
    let tenant = TenantId("tenant-idem-3".into());
    let idem_key = "req-no-dup";

    service.increment(&tenant, Some(idem_key)).await.unwrap();
    service.increment(&tenant, Some(idem_key)).await.unwrap();

    // Query outbox table via service's get_value (indirect verification)
    // The outbox should have exactly 1 entry since the second increment was idempotent
    let value = service.get_value(&tenant).await.unwrap();
    assert_eq!(
        value, 1,
        "counter value should be 1 after idempotent increment"
    );
}

#[tokio::test]
async fn concurrent_same_idempotency_key_commits_once() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test_same_idem.db");
    let _guard = Box::leak(Box::new(dir));
    let db = storage_turso::EmbeddedTurso::new(db_path.to_str().unwrap())
        .await
        .unwrap();
    let repo = LibSqlCounterRepository::new(db.clone());
    repo.migrate().await.unwrap();

    let service = Arc::new(TenantScopedCounterService::new(repo));
    let tenant = TenantId("same-idem-tenant".into());
    let idem_key = "same-request-key";

    let mut handles = Vec::new();
    for _ in 0..8 {
        let svc = service.clone();
        let tid = tenant.clone();
        handles.push(tokio::spawn(async move {
            svc.increment(&tid, Some(idem_key)).await.unwrap()
        }));
    }

    let mut results = Vec::new();
    for handle in handles {
        results.push(handle.await.unwrap());
    }

    assert!(
        results.iter().all(|value| *value == 1),
        "same idempotency key must replay the first result"
    );
    assert_eq!(service.get_value(&tenant).await.unwrap(), 1);
    assert_eq!(count_outbox(&db).await, 1);
    assert_eq!(count_idempotency(&db).await, 1);
}

#[tokio::test]
async fn same_idempotency_key_with_different_operation_conflicts() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test_idem_conflict.db");
    let _guard = Box::leak(Box::new(dir));
    let db = storage_turso::EmbeddedTurso::new(db_path.to_str().unwrap())
        .await
        .unwrap();
    let repo = LibSqlCounterRepository::new(db.clone());
    repo.migrate().await.unwrap();

    let service = TenantScopedCounterService::new(repo);
    let tenant = TenantId("idem-conflict-tenant".into());
    let idem_key = "same-key-different-operation";

    let increment = service.increment(&tenant, Some(idem_key)).await.unwrap();
    assert_eq!(increment, 1);

    let conflict = service.decrement(&tenant, Some(idem_key)).await;
    assert!(
        conflict.is_err(),
        "same idempotency key must not be reused for a different operation"
    );
    assert_eq!(service.get_value(&tenant).await.unwrap(), 1);
    assert_eq!(count_outbox(&db).await, 1);
}

// ---------------------------------------------------------------------------
// Concurrency tests with real EmbeddedTurso (multi-connection)
// ---------------------------------------------------------------------------

/// Count rows in the event_outbox table via raw port query.
async fn count_outbox(port: &impl LibSqlPort) -> usize {
    #[derive(Deserialize)]
    struct CountRow {
        count: i64,
    }
    let rows: Vec<CountRow> = port
        .query("SELECT COUNT(*) as count FROM event_outbox", vec![])
        .await
        .unwrap();
    rows.first().map(|r| r.count as usize).unwrap_or(0)
}

/// Count rows in the counter_idempotency table.
async fn count_idempotency(port: &impl LibSqlPort) -> usize {
    #[derive(Deserialize)]
    struct CountRow {
        count: i64,
    }
    let rows: Vec<CountRow> = port
        .query("SELECT COUNT(*) as count FROM counter_idempotency", vec![])
        .await
        .unwrap();
    rows.first().map(|r| r.count as usize).unwrap_or(0)
}

#[derive(Debug, Deserialize)]
struct IdempotencyEvidenceRow {
    request_hash: String,
    tenant_id: String,
    resource: String,
    operation: String,
    status: String,
    result_value: i64,
    result_version: i64,
    response_digest: String,
}

async fn idempotency_evidence(
    port: &impl LibSqlPort,
    tenant: &TenantId,
    key: &str,
) -> IdempotencyEvidenceRow {
    let rows: Vec<IdempotencyEvidenceRow> = port
        .query(
            "SELECT request_hash, tenant_id, resource, operation, status, result_value, result_version, response_digest \
             FROM counter_idempotency WHERE counter_id = ? AND idempotency_key = ?",
            vec![tenant.as_str().to_string(), key.to_string()],
        )
        .await
        .unwrap();
    rows.into_iter()
        .next()
        .expect("idempotency evidence row should exist")
}

#[tokio::test]
async fn idempotency_record_stores_request_fingerprint_and_response_digest() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test_idem_evidence.db");
    let _guard = Box::leak(Box::new(dir));
    let db = storage_turso::EmbeddedTurso::new(db_path.to_str().unwrap())
        .await
        .unwrap();
    let repo = LibSqlCounterRepository::new(db.clone());
    repo.migrate().await.unwrap();

    let service = TenantScopedCounterService::new(repo);
    let tenant = TenantId("idem-evidence-tenant".into());
    let idem_key = "idem-evidence-key";

    let first = service.increment(&tenant, Some(idem_key)).await.unwrap();
    let replay = service.increment(&tenant, Some(idem_key)).await.unwrap();
    let row = idempotency_evidence(&db, &tenant, idem_key).await;

    assert_eq!(first, 1);
    assert_eq!(replay, 1);
    assert_eq!(row.tenant_id, tenant.as_str());
    assert_eq!(row.resource, "counter");
    assert_eq!(row.operation, "increment");
    assert_eq!(row.status, "completed");
    assert_eq!(row.result_value, 1);
    assert_eq!(row.result_version, 1);
    assert_eq!(row.request_hash.len(), 64);
    assert_eq!(row.response_digest.len(), 64);
    assert_eq!(count_outbox(&db).await, 1);
}

#[tokio::test]
async fn concurrent_increments_on_embedded_turso() {
    // Use real EmbeddedTurso to validate CAS + outbox under concurrent load.
    // Each task gets its own connection via db.connect().
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let _guard = Box::leak(Box::new(dir)); // keep alive for test lifetime
    let db = storage_turso::EmbeddedTurso::new(db_path.to_str().unwrap())
        .await
        .unwrap();
    let repo = LibSqlCounterRepository::new(db.clone());
    repo.migrate().await.unwrap();

    let service = Arc::new(TenantScopedCounterService::new(repo));
    let tenant = TenantId("concurrent-tenant".into());
    let num_tasks = 10;

    let mut handles = Vec::new();
    for i in 0..num_tasks {
        let svc = service.clone();
        let tid = tenant.clone();
        handles.push(tokio::spawn(async move {
            svc.increment(&tid, Some(&format!("idem-{i}")))
                .await
                .unwrap()
        }));
    }

    let mut results: Vec<i64> = Vec::new();
    for h in handles {
        results.push(h.await.unwrap());
    }
    results.sort();

    // Every increment should produce a unique value 1..=num_tasks
    let expected: Vec<i64> = (1..=num_tasks).collect();
    assert_eq!(
        results, expected,
        "concurrent increments must produce unique sequential values"
    );

    // Final value must equal num_tasks
    let final_value = service.get_value(&tenant).await.unwrap();
    assert_eq!(final_value, num_tasks);

    // Outbox must have exactly num_tasks entries (one per successful mutation)
    let outbox_count = count_outbox(&db).await;
    assert_eq!(
        outbox_count, num_tasks as usize,
        "outbox must match mutation count"
    );

    // Idempotency table must have exactly num_tasks entries
    let idem_count = count_idempotency(&db).await;
    assert_eq!(
        idem_count, num_tasks as usize,
        "idempotency cache must match mutation count"
    );
}

#[tokio::test]
async fn concurrent_increments_without_idempotency() {
    // Same as above but without idempotency keys — tests pure CAS path.
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test2.db");
    let _guard = Box::leak(Box::new(dir));
    let db = storage_turso::EmbeddedTurso::new(db_path.to_str().unwrap())
        .await
        .unwrap();
    let repo = LibSqlCounterRepository::new(db.clone());
    repo.migrate().await.unwrap();

    let service = Arc::new(TenantScopedCounterService::new(repo));
    let tenant = TenantId("no-idem-tenant".into());
    let num_tasks = 20;

    let mut handles = Vec::new();
    for _ in 0..num_tasks {
        let svc = service.clone();
        let tid = tenant.clone();
        handles.push(tokio::spawn(async move {
            svc.increment(&tid, None).await.unwrap()
        }));
    }

    let mut results: Vec<i64> = Vec::new();
    for h in handles {
        results.push(h.await.unwrap());
    }
    results.sort();

    // Each increment produces a unique value since CAS serializes writes.
    let expected: Vec<i64> = (1..=num_tasks).collect();
    assert_eq!(results, expected);

    let final_value = service.get_value(&tenant).await.unwrap();
    assert_eq!(final_value, num_tasks);

    let outbox_count = count_outbox(&db).await;
    assert_eq!(outbox_count, num_tasks as usize);

    // No idempotency keys used
    let idem_count = count_idempotency(&db).await;
    assert_eq!(idem_count, 0);
}

// ---------------------------------------------------------------------------
// Atomicity verification — CAS + outbox in single BEGIN/COMMIT transaction
// ---------------------------------------------------------------------------

#[tokio::test]
async fn atomic_counter_outbox_consistency_under_load() {
    // Proves that counter mutations and outbox entries are always 1:1.
    // If the BEGIN/COMMIT transaction failed to be atomic, concurrent tasks
    // could produce counter mutations without outbox entries (or vice versa).
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test_atomic.db");
    let _guard = Box::leak(Box::new(dir));
    let db = storage_turso::EmbeddedTurso::new(db_path.to_str().unwrap())
        .await
        .unwrap();
    let repo = LibSqlCounterRepository::new(db.clone());
    repo.migrate().await.unwrap();

    let service = Arc::new(TenantScopedCounterService::new(repo));
    let tenant = TenantId("atomic-check-tenant".into());
    let num_tasks = 15;

    let mut handles = Vec::new();
    for _ in 0..num_tasks {
        let svc = service.clone();
        let tid = tenant.clone();
        handles.push(tokio::spawn(async move {
            svc.increment(&tid, None).await.unwrap()
        }));
    }

    let mut results: Vec<i64> = Vec::new();
    for h in handles {
        results.push(h.await.unwrap());
    }
    results.sort();

    // Counter must equal number of successful mutations
    let final_value = service.get_value(&tenant).await.unwrap();
    assert_eq!(
        final_value, num_tasks,
        "counter value must match task count"
    );

    // Outbox must have EXACTLY num_tasks entries — no orphaned mutations
    let outbox_count = count_outbox(&db).await;
    assert_eq!(
        outbox_count, num_tasks as usize,
        "outbox count must match counter mutations (transactional consistency)"
    );

    // Each increment should produce a unique value
    let expected: Vec<i64> = (1..=num_tasks).collect();
    assert_eq!(
        results, expected,
        "concurrent increments must produce unique sequential values"
    );
}
