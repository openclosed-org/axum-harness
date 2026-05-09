//! SurrealDB implementation of TenantRepository.
//!
//! Translates the abstract TenantRepository trait into concrete SurrealQL operations.

use std::collections::BTreeMap;

use async_trait::async_trait;
use data::ports::surreal_db::{
    SurrealDbPort, SurrealFieldValue, SurrealOrderDirection, TenantQueryOperation,
};
use serde::Deserialize;

use crate::domain::{CreateTenantInput, Tenant};
use crate::ports::{RepositoryError, TenantRepository, UserTenantBinding};

/// Raw row shape from the tenant table.
#[derive(Debug, Deserialize)]
struct TenantRow {
    #[serde(rename = "tenant_key")]
    id: String,
    name: String,
    created_at: String,
}

/// SurrealDB-backed TenantRepository.
pub struct SurrealDbTenantRepository<P: SurrealDbPort> {
    port: P,
}

impl<P: SurrealDbPort> SurrealDbTenantRepository<P> {
    pub fn new(port: P) -> Self {
        Self { port }
    }
}

#[async_trait]
impl<P: SurrealDbPort> TenantRepository for SurrealDbTenantRepository<P> {
    async fn create_tenant(&self, input: CreateTenantInput) -> Result<Tenant, RepositoryError> {
        let mut vars: BTreeMap<String, SurrealFieldValue> = BTreeMap::new();
        vars.insert("tenant_key".into(), serde_json::json!(input.id).into());
        vars.insert("name".into(), serde_json::json!(input.name).into());

        let rows: Vec<TenantRow> = self
            .port
            .tenant_query(TenantQueryOperation::create("tenant", vars)?)
            .await?;

        rows.into_iter()
            .next()
            .map(row_to_tenant)
            .ok_or_else(|| RepositoryError::from("Failed to create tenant"))
    }

    async fn get_tenant(&self, id: &str) -> Result<Option<Tenant>, RepositoryError> {
        let mut vars: BTreeMap<String, SurrealFieldValue> = BTreeMap::new();
        vars.insert("tenant_key".into(), serde_json::json!(id).into());

        let rows: Vec<TenantRow> = self
            .port
            .tenant_query(TenantQueryOperation::select(
                "tenant",
                Vec::new(),
                vars,
                None,
                None,
            )?)
            .await?;

        Ok(rows.into_iter().next().map(row_to_tenant))
    }

    async fn list_tenants(&self) -> Result<Vec<Tenant>, RepositoryError> {
        let rows: Vec<TenantRow> = self
            .port
            .tenant_query(TenantQueryOperation::select(
                "tenant",
                Vec::new(),
                BTreeMap::new(),
                Some(("created_at".into(), SurrealOrderDirection::Desc)),
                None,
            )?)
            .await?;
        Ok(rows.into_iter().map(row_to_tenant).collect())
    }

    async fn delete_tenant(&self, id: &str) -> Result<(), RepositoryError> {
        let mut vars: BTreeMap<String, SurrealFieldValue> = BTreeMap::new();
        vars.insert("tenant_key".into(), serde_json::json!(id).into());

        let _: Vec<serde_json::Value> = self
            .port
            .tenant_query(TenantQueryOperation::delete("tenant", vars)?)
            .await?;

        Ok(())
    }

    async fn find_user_tenant(
        &self,
        user_sub: &str,
    ) -> Result<Option<UserTenantBinding>, RepositoryError> {
        #[derive(Debug, Deserialize)]
        struct BindingRow {
            tenant_id: String,
            role: String,
        }

        let mut vars: BTreeMap<String, SurrealFieldValue> = BTreeMap::new();
        vars.insert("user_sub".into(), serde_json::json!(user_sub).into());

        let rows: Vec<BindingRow> = self
            .port
            .tenant_query(TenantQueryOperation::select(
                "user_tenant",
                vec!["tenant_id".into(), "role".into()],
                vars,
                None,
                Some(1),
            )?)
            .await?;

        Ok(rows.into_iter().next().map(|row| UserTenantBinding {
            tenant_id: row.tenant_id,
            role: row.role,
        }))
    }

    async fn create_user_tenant_binding(
        &self,
        user_sub: &str,
        tenant_id: &str,
        role: &str,
    ) -> Result<(), RepositoryError> {
        let _ = tenant_id;
        let mut vars: BTreeMap<String, SurrealFieldValue> = BTreeMap::new();
        vars.insert("user_sub".into(), serde_json::json!(user_sub).into());
        vars.insert("tenant_id".into(), serde_json::json!(tenant_id).into());
        vars.insert("role".into(), serde_json::json!(role).into());

        let _: Vec<serde_json::Value> = self
            .port
            .tenant_query(TenantQueryOperation::create("user_tenant", vars)?)
            .await?;

        Ok(())
    }
}

fn row_to_tenant(row: TenantRow) -> Tenant {
    Tenant {
        id: row.id,
        name: row.name,
        created_at: row.created_at,
    }
}
