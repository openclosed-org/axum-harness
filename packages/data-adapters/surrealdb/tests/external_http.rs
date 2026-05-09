use data_traits::ports::surreal_db::{
    SurrealAdminMarker, SurrealDbPort, SurrealFieldValue, TenantQueryOperation,
};
use serde::Deserialize;
use std::collections::BTreeMap;
use storage_surrealdb::{ExternalSurrealDb, TENANT_MIGRATIONS};

#[derive(Debug, Deserialize)]
struct TenantRow {
    #[serde(rename = "tenant_key")]
    id: String,
    name: String,
    tenant_scope: String,
}

#[tokio::test]
#[ignore = "requires a local external SurrealDB server"]
async fn external_http_server_executes_tenant_scoped_queries() {
    let endpoint =
        std::env::var("SURREALDB_URL").unwrap_or_else(|_| "http://127.0.0.1:8000".to_string());
    let namespace = std::env::var("SURREALDB_NS").unwrap_or_else(|_| "axh".to_string());
    let database = std::env::var("SURREALDB_DB").unwrap_or_else(|_| "main".to_string());
    let username = std::env::var("SURREALDB_USER").unwrap_or_else(|_| "root".to_string());
    let password = std::env::var("SURREALDB_PASS").unwrap_or_else(|_| "root".to_string());
    let tenant_id = format!("tenant-local-{}", std::process::id());

    let admin = ExternalSurrealDb::new_with_auth(
        endpoint.clone(),
        namespace.clone(),
        database.clone(),
        username.clone(),
        password.clone(),
        None,
    );
    admin.health_check().await.unwrap();
    for (_, sql) in TENANT_MIGRATIONS {
        admin
            .unsafe_admin_query::<serde_json::Value>(SurrealAdminMarker::unsafe_admin(), sql)
            .await
            .unwrap();
    }

    let tenant_db = ExternalSurrealDb::new_with_auth(
        endpoint,
        namespace,
        database,
        username,
        password,
        Some(tenant_id.clone()),
    );

    let created = tenant_db
        .tenant_query::<TenantRow>(
            TenantQueryOperation::create(
                "tenant",
                BTreeMap::from([
                    (
                        "tenant_key".to_string(),
                        SurrealFieldValue::from(serde_json::json!(tenant_id.clone())),
                    ),
                    (
                        "name".to_string(),
                        SurrealFieldValue::from(serde_json::json!("Local Tenant")),
                    ),
                ]),
            )
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(created[0].id, tenant_id);
    assert_eq!(created[0].tenant_scope, tenant_id);
    assert_eq!(created[0].name, "Local Tenant");

    let selected = tenant_db
        .tenant_query::<TenantRow>(
            TenantQueryOperation::select(
                "tenant",
                vec![
                    "tenant_key".to_string(),
                    "tenant_scope".to_string(),
                    "name".to_string(),
                ],
                BTreeMap::from([(
                    "name".to_string(),
                    SurrealFieldValue::from(serde_json::json!("Local Tenant")),
                )]),
                None,
                Some(1),
            )
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(selected.len(), 1);
    assert_eq!(selected[0].id, tenant_id);
    assert_eq!(selected[0].tenant_scope, tenant_id);

    tenant_db
        .tenant_query::<serde_json::Value>(
            TenantQueryOperation::delete(
                "tenant",
                BTreeMap::from([(
                    "name".to_string(),
                    SurrealFieldValue::from(serde_json::json!("Local Tenant")),
                )]),
            )
            .unwrap(),
        )
        .await
        .unwrap();
}
