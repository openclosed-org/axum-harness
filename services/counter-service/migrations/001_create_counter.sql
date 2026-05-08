-- Counter service migrations
-- Tables: counter (with CAS version), counter_idempotency
-- All migrations are idempotent (CREATE TABLE IF NOT EXISTS).

CREATE TABLE IF NOT EXISTS counter (
    tenant_id TEXT PRIMARY KEY,
    value INTEGER NOT NULL DEFAULT 0,
    version INTEGER NOT NULL DEFAULT 0,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS counter_idempotency (
    counter_id TEXT NOT NULL,
    idempotency_key TEXT NOT NULL,
    request_hash TEXT NOT NULL,
    tenant_id TEXT NOT NULL DEFAULT '',
    resource TEXT NOT NULL DEFAULT 'counter',
    operation TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'in_progress',
    result_value INTEGER,
    result_version INTEGER,
    response_digest TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    completed_at TEXT,
    PRIMARY KEY (counter_id, idempotency_key)
);
