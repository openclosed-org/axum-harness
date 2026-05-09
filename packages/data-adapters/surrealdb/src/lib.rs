//! SurrealDB external database adapter.
//!
//! The default transport targets an external SurrealDB server over HTTP. This
//! keeps local, single-VPS, and k3s deployments aligned with the normal
//! database-server model and avoids compiling SurrealDB's Rust SDK in default
//! checks.

use async_trait::async_trait;
use data_traits::ports::surreal_db::{
    SurrealAdminMarker, SurrealDbPort, SurrealError, TenantQueryOperation,
};
use kernel::TenantId;
use serde::Deserialize;
use serde::de::DeserializeOwned;

#[cfg(feature = "http")]
mod http_transport {
    use super::*;

    pub const DEFAULT_NAMESPACE: &str = "axh";
    pub const DEFAULT_DATABASE: &str = "main";
    pub const DEFAULT_TENANT_SCOPE: &str = "platform";

    #[derive(Debug, Clone)]
    pub struct ExternalSurrealDb {
        client: reqwest::Client,
        endpoint: String,
        namespace: String,
        database: String,
        username: String,
        password: String,
        tenant_id: Option<String>,
    }

    impl ExternalSurrealDb {
        pub fn new(endpoint: impl Into<String>, tenant_id: TenantId) -> Self {
            Self::new_with_auth(
                endpoint,
                DEFAULT_NAMESPACE,
                DEFAULT_DATABASE,
                "root",
                "root",
                Some(tenant_id.0),
            )
        }

        pub fn new_admin(endpoint: impl Into<String>) -> Self {
            Self::new_with_auth(
                endpoint,
                DEFAULT_NAMESPACE,
                DEFAULT_DATABASE,
                "root",
                "root",
                None,
            )
        }

        pub fn new_with_auth(
            endpoint: impl Into<String>,
            namespace: impl Into<String>,
            database: impl Into<String>,
            username: impl Into<String>,
            password: impl Into<String>,
            tenant_id: Option<String>,
        ) -> Self {
            Self {
                client: reqwest::Client::new(),
                endpoint: endpoint.into().trim_end_matches('/').to_string(),
                namespace: namespace.into(),
                database: database.into(),
                username: username.into(),
                password: password.into(),
                tenant_id,
            }
        }

        pub fn tenant_id(&self) -> Option<&str> {
            self.tenant_id.as_deref()
        }

        async fn execute_sql<T: DeserializeOwned + Send + Sync>(
            &self,
            sql: &str,
        ) -> Result<Vec<T>, SurrealError> {
            let response = self
                .client
                .post(format!("{}/sql", self.endpoint))
                .basic_auth(&self.username, Some(&self.password))
                .header("Surreal-NS", &self.namespace)
                .header("Surreal-DB", &self.database)
                .header(reqwest::header::ACCEPT, "application/json")
                .body(sql.to_string())
                .send()
                .await?;

            let status = response.status();
            let body = response.text().await?;
            if !status.is_success() {
                return Err(Box::new(ExternalSurrealError::Http { status, body }));
            }

            let results: Vec<SurrealSqlResult> = serde_json::from_str(&body)?;
            if results.is_empty() {
                return Ok(Vec::new());
            }

            for result in &results {
                if result.status == "OK" {
                    continue;
                }
                return Err(Box::new(ExternalSurrealError::Query {
                    detail: result.error_detail(),
                }));
            }

            let Some(value) = results.into_iter().rev().find_map(|result| result.result) else {
                return Ok(Vec::new());
            };
            if value.is_null() {
                return Ok(Vec::new());
            }
            Ok(serde_json::from_value(value)?)
        }
    }

    #[async_trait]
    impl SurrealDbPort for ExternalSurrealDb {
        async fn health_check(&self) -> Result<(), SurrealError> {
            let response = self
                .client
                .get(format!("{}/health", self.endpoint))
                .send()
                .await?;
            if !response.status().is_success() {
                return Err(Box::new(ExternalSurrealError::Health(response.status())));
            }
            Ok(())
        }

        async fn tenant_query<T: DeserializeOwned + Send + Sync>(
            &self,
            operation: TenantQueryOperation,
        ) -> Result<Vec<T>, SurrealError> {
            let Some(tenant_id) = self.tenant_id() else {
                return Err(Box::new(ExternalSurrealError::MissingTenant));
            };
            let sql = operation.to_surrealql(tenant_id)?;
            self.execute_sql(&sql).await
        }

        async fn unsafe_admin_query<T: DeserializeOwned + Send + Sync>(
            &self,
            _marker: SurrealAdminMarker,
            sql: &str,
        ) -> Result<Vec<T>, SurrealError> {
            self.execute_sql(sql).await
        }
    }

    #[derive(Debug, Deserialize)]
    struct SurrealSqlResult {
        status: String,
        result: Option<serde_json::Value>,
        detail: Option<String>,
    }

    impl SurrealSqlResult {
        fn error_detail(&self) -> String {
            self.detail.clone().unwrap_or_else(|| match &self.result {
                Some(value) if !value.is_null() => value.to_string(),
                _ => "SurrealDB query failed".to_string(),
            })
        }
    }

    #[derive(Debug)]
    enum ExternalSurrealError {
        Health(reqwest::StatusCode),
        Http {
            status: reqwest::StatusCode,
            body: String,
        },
        Query {
            detail: String,
        },
        MissingTenant,
    }

    impl std::fmt::Display for ExternalSurrealError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                Self::Health(status) => write!(f, "SurrealDB health check failed: {status}"),
                Self::Http { status, body } => write!(f, "SurrealDB HTTP error {status}: {body}"),
                Self::Query { detail } => write!(f, "SurrealDB query error: {detail}"),
                Self::MissingTenant => f.write_str("tenant query requires a tenant-scoped adapter"),
            }
        }
    }

    impl std::error::Error for ExternalSurrealError {}
}

#[cfg(feature = "http")]
pub use http_transport::ExternalSurrealDb;

/// Default adapter name for the external-server SurrealDB database provider.
#[cfg(feature = "http")]
pub type TenantAwareSurrealDb = ExternalSurrealDb;

pub const DEFAULT_NAMESPACE: &str = http_transport::DEFAULT_NAMESPACE;
pub const DEFAULT_DATABASE: &str = http_transport::DEFAULT_DATABASE;
pub const DEFAULT_TENANT_SCOPE: &str = http_transport::DEFAULT_TENANT_SCOPE;

const TENANT_TABLE_MIGRATION: &str = "DEFINE TABLE IF NOT EXISTS tenant SCHEMAFULL;\nDEFINE FIELD IF NOT EXISTS tenant_scope ON TABLE tenant TYPE string;\nDEFINE FIELD IF NOT EXISTS tenant_key ON TABLE tenant TYPE string;\nDEFINE FIELD IF NOT EXISTS name ON TABLE tenant TYPE string;\nDEFINE FIELD IF NOT EXISTS created_at ON TABLE tenant TYPE datetime DEFAULT time::now();\nDEFINE INDEX IF NOT EXISTS tenant_scope_key_idx ON TABLE tenant COLUMNS tenant_scope, tenant_key UNIQUE;";

const USER_TABLE_MIGRATION: &str = "DEFINE TABLE IF NOT EXISTS user SCHEMAFULL;\nDEFINE FIELD IF NOT EXISTS tenant_scope ON TABLE user TYPE string;\nDEFINE FIELD IF NOT EXISTS user_key ON TABLE user TYPE string;\nDEFINE FIELD IF NOT EXISTS user_sub ON TABLE user TYPE string;\nDEFINE FIELD IF NOT EXISTS display_name ON TABLE user TYPE string;\nDEFINE FIELD IF NOT EXISTS email ON TABLE user TYPE option<string>;\nDEFINE FIELD IF NOT EXISTS created_at ON TABLE user TYPE datetime DEFAULT time::now();\nDEFINE FIELD IF NOT EXISTS last_login_at ON TABLE user TYPE option<datetime>;\nDEFINE INDEX IF NOT EXISTS user_scope_sub_idx ON TABLE user COLUMNS tenant_scope, user_sub UNIQUE;\nDEFINE INDEX IF NOT EXISTS user_scope_key_idx ON TABLE user COLUMNS tenant_scope, user_key UNIQUE;";

const USER_TENANT_TABLE_MIGRATION: &str = "DEFINE TABLE IF NOT EXISTS user_tenant SCHEMAFULL;\nDEFINE FIELD IF NOT EXISTS tenant_scope ON TABLE user_tenant TYPE string;\nDEFINE FIELD IF NOT EXISTS tenant_id ON TABLE user_tenant TYPE string;\nDEFINE FIELD IF NOT EXISTS user_sub ON TABLE user_tenant TYPE string;\nDEFINE FIELD IF NOT EXISTS role ON TABLE user_tenant TYPE string DEFAULT 'member';\nDEFINE FIELD IF NOT EXISTS joined_at ON TABLE user_tenant TYPE datetime DEFAULT time::now();\nDEFINE INDEX IF NOT EXISTS user_tenant_lookup ON TABLE user_tenant COLUMNS tenant_scope, user_sub UNIQUE;\nDEFINE INDEX IF NOT EXISTS user_tenant_tenant_idx ON TABLE user_tenant COLUMNS tenant_scope, tenant_id;";

const COUNTER_TABLE_MIGRATION: &str = "DEFINE TABLE IF NOT EXISTS counter SCHEMAFULL;\nDEFINE FIELD IF NOT EXISTS tenant_scope ON TABLE counter TYPE string;\nDEFINE FIELD IF NOT EXISTS counter_key ON TABLE counter TYPE string;\nDEFINE FIELD IF NOT EXISTS value ON TABLE counter TYPE int DEFAULT 0;\nDEFINE FIELD IF NOT EXISTS version ON TABLE counter TYPE int DEFAULT 0;\nDEFINE FIELD IF NOT EXISTS updated_at ON TABLE counter TYPE datetime DEFAULT time::now();\nDEFINE INDEX IF NOT EXISTS counter_scope_key_idx ON TABLE counter COLUMNS tenant_scope, counter_key UNIQUE;\nDEFINE TABLE IF NOT EXISTS counter_idempotency SCHEMAFULL;\nDEFINE FIELD IF NOT EXISTS tenant_scope ON TABLE counter_idempotency TYPE string;\nDEFINE FIELD IF NOT EXISTS counter_key ON TABLE counter_idempotency TYPE string;\nDEFINE FIELD IF NOT EXISTS idempotency_key ON TABLE counter_idempotency TYPE string;\nDEFINE FIELD IF NOT EXISTS request_hash ON TABLE counter_idempotency TYPE string;\nDEFINE FIELD IF NOT EXISTS operation ON TABLE counter_idempotency TYPE string;\nDEFINE FIELD IF NOT EXISTS status ON TABLE counter_idempotency TYPE string;\nDEFINE FIELD IF NOT EXISTS result_value ON TABLE counter_idempotency TYPE option<int>;\nDEFINE FIELD IF NOT EXISTS result_version ON TABLE counter_idempotency TYPE option<int>;\nDEFINE FIELD IF NOT EXISTS completed_at ON TABLE counter_idempotency TYPE option<datetime>;\nDEFINE INDEX IF NOT EXISTS counter_idempotency_scope_key_idx ON TABLE counter_idempotency COLUMNS tenant_scope, counter_key, idempotency_key UNIQUE;";

const EVENT_OUTBOX_TABLE_MIGRATION: &str = "DEFINE TABLE IF NOT EXISTS event_outbox SCHEMAFULL;\nDEFINE FIELD IF NOT EXISTS tenant_scope ON TABLE event_outbox TYPE string;\nDEFINE FIELD IF NOT EXISTS event_id ON TABLE event_outbox TYPE string;\nDEFINE FIELD IF NOT EXISTS event_type ON TABLE event_outbox TYPE string;\nDEFINE FIELD IF NOT EXISTS event_payload ON TABLE event_outbox TYPE string;\nDEFINE FIELD IF NOT EXISTS source_service ON TABLE event_outbox TYPE string;\nDEFINE FIELD IF NOT EXISTS correlation_id ON TABLE event_outbox TYPE option<string>;\nDEFINE FIELD IF NOT EXISTS status ON TABLE event_outbox TYPE string DEFAULT 'pending';\nDEFINE FIELD IF NOT EXISTS created_at ON TABLE event_outbox TYPE datetime DEFAULT time::now();\nDEFINE INDEX IF NOT EXISTS event_outbox_event_id_idx ON TABLE event_outbox COLUMNS tenant_scope, event_id UNIQUE;";

const GRAPH_AND_LIVE_MIGRATION: &str = "DEFINE TABLE IF NOT EXISTS tenant_edge SCHEMAFULL TYPE RELATION IN tenant OUT tenant;\nDEFINE FIELD IF NOT EXISTS tenant_scope ON TABLE tenant_edge TYPE string;\nDEFINE INDEX IF NOT EXISTS tenant_edge_idx ON TABLE tenant_edge COLUMNS tenant_scope;\nDEFINE TABLE IF NOT EXISTS live_event SCHEMAFULL;\nDEFINE FIELD IF NOT EXISTS tenant_scope ON TABLE live_event TYPE string;\nDEFINE FIELD IF NOT EXISTS payload ON TABLE live_event TYPE object FLEXIBLE;\nDEFINE INDEX IF NOT EXISTS live_event_scope_idx ON TABLE live_event COLUMNS tenant_scope;";

/// Versioned migration statements for the SurrealDB database provider.
pub const TENANT_MIGRATIONS: &[(&str, &str)] = &[
    ("0001_tenant_tables", TENANT_TABLE_MIGRATION),
    ("0002_user_tables", USER_TABLE_MIGRATION),
    ("0003_user_tenant_bindings", USER_TENANT_TABLE_MIGRATION),
    ("0004_counter_tables", COUNTER_TABLE_MIGRATION),
    ("0005_event_outbox", EVENT_OUTBOX_TABLE_MIGRATION),
    ("0006_graph_and_live_boundaries", GRAPH_AND_LIVE_MIGRATION),
];

pub fn migration_dry_run() -> Vec<MigrationPreview> {
    TENANT_MIGRATIONS
        .iter()
        .map(|(version, sql)| MigrationPreview {
            version: (*version).to_string(),
            sql: (*sql).to_string(),
        })
        .collect()
}

pub fn backup_command(endpoint: &str, namespace: &str, database: &str, output: &str) -> String {
    format!(
        "surreal export --conn {endpoint} --ns {namespace} --db {database} --user root --pass root {output}"
    )
}

pub fn restore_command(endpoint: &str, namespace: &str, database: &str, input: &str) -> String {
    format!(
        "surreal import --conn {endpoint} --ns {namespace} --db {database} --user root --pass root {input}"
    )
}

pub fn restore_verification_query() -> TenantQueryOperation {
    TenantQueryOperation::select("tenant", Vec::new(), Default::default(), None, Some(1))
        .expect("static restore verification query is valid")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationPreview {
    pub version: String,
    pub sql: String,
}

#[cfg(feature = "sdk")]
pub mod sdk {
    //! Optional SDK lane. This module intentionally has no default implementation
    //! so default checks do not compile SurrealDB's Rust SDK.
    pub use surrealdb;
}

#[cfg(test)]
mod tests {
    use super::*;
    use data_traits::ports::surreal_db::{
        SurrealFieldValue, SurrealOrderDirection, SurrealQueryBuildError,
    };
    use std::collections::BTreeMap;

    #[test]
    fn tenant_select_builds_tenant_scoped_query() {
        let operation = TenantQueryOperation::select(
            "tenant",
            vec!["id".into(), "name".into()],
            BTreeMap::from([(
                "tenant_key".to_string(),
                serde_json::json!("tenant-a").into(),
            )]),
            Some(("created_at".into(), SurrealOrderDirection::Desc)),
            Some(1),
        )
        .unwrap();

        let sql = operation.to_surrealql("tenant-a").unwrap();

        assert!(sql.contains("tenant_scope = \"tenant-a\""));
        assert!(sql.contains("tenant_key = \"tenant-a\""));
        assert!(sql.contains("ORDER BY created_at DESC"));
        assert!(!sql.contains("$tenant_scope"));
    }

    #[test]
    fn tenant_create_rejects_caller_supplied_scope() {
        let err = TenantQueryOperation::create(
            "tenant",
            BTreeMap::from([("tenant_scope".to_string(), serde_json::json!("evil").into())]),
        )
        .unwrap_err();

        assert_eq!(err, SurrealQueryBuildError::ReservedTenantField);
    }

    #[test]
    fn graph_traversal_keeps_edge_and_target_tenant_scoped() {
        let sql = TenantQueryOperation::graph_traverse(
            "tenant",
            serde_json::json!("tenant-a"),
            "tenant_edge",
            "tenant",
        )
        .unwrap()
        .to_surrealql("tenant-a")
        .unwrap();

        assert!(sql.contains("->tenant_edge[WHERE tenant_scope = \"tenant-a\"]"));
        assert!(sql.contains("->tenant[WHERE tenant_scope = \"tenant-a\"]"));
        assert!(sql.ends_with("WHERE tenant_scope = \"tenant-a\""));
    }

    #[test]
    fn live_query_is_tenant_scoped() {
        let sql = TenantQueryOperation::live_table("live_event", BTreeMap::new())
            .unwrap()
            .to_surrealql("tenant-a")
            .unwrap();

        assert_eq!(
            sql,
            "LIVE SELECT * FROM live_event WHERE tenant_scope = \"tenant-a\""
        );
    }

    #[test]
    fn migration_dry_run_is_versioned() {
        let preview = migration_dry_run();

        assert_eq!(preview.len(), 6);
        assert_eq!(preview[0].version, "0001_tenant_tables");
        assert!(preview[0].sql.contains("DEFINE TABLE IF NOT EXISTS tenant"));
        assert_eq!(preview[3].version, "0004_counter_tables");
        assert!(
            preview[3]
                .sql
                .contains("DEFINE TABLE IF NOT EXISTS counter")
        );
    }

    #[test]
    fn restore_verification_is_tenant_typed() {
        let sql = restore_verification_query()
            .to_surrealql("tenant-a")
            .unwrap();

        assert_eq!(
            sql,
            "SELECT * FROM tenant WHERE tenant_scope = \"tenant-a\" LIMIT 1"
        );
    }

    #[test]
    fn time_now_is_rendered_as_surrealql_function() {
        let sql = TenantQueryOperation::create(
            "tenant",
            BTreeMap::from([("created_at".to_string(), SurrealFieldValue::TimeNow)]),
        )
        .unwrap()
        .to_surrealql("tenant-a")
        .unwrap();

        assert!(sql.contains("created_at: time::now()"));
    }
}
