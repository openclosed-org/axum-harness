//! SurrealDB implementations of user-service repository ports.

use std::collections::BTreeMap;

use async_trait::async_trait;
use chrono::Utc;
use data::ports::surreal_db::{SurrealDbPort, SurrealFieldValue, TenantQueryOperation};
use serde::Deserialize;

use crate::domain;
use crate::domain::UserTenantBinding;
use crate::domain::error::UserError;
use crate::ports::{TenantRepository, UserRepository, UserTenantRepository};

impl From<data::ports::surreal_db::SurrealQueryBuildError> for UserError {
    fn from(error: data::ports::surreal_db::SurrealQueryBuildError) -> Self {
        UserError::Database(error.to_string())
    }
}

pub struct SurrealDbUserRepository<P: SurrealDbPort> {
    db: P,
}

impl<P: SurrealDbPort> SurrealDbUserRepository<P> {
    pub fn new(db: P) -> Self {
        Self { db }
    }
}

#[derive(Debug, Deserialize)]
struct UserRow {
    #[serde(rename = "user_key")]
    id: String,
    user_sub: String,
    display_name: String,
    email: Option<String>,
    created_at: String,
    last_login_at: Option<String>,
}

#[async_trait]
impl<P: SurrealDbPort> UserRepository for SurrealDbUserRepository<P> {
    async fn find_by_sub(&self, user_sub: &str) -> Result<Option<domain::User>, UserError> {
        let mut filters = BTreeMap::new();
        filters.insert("user_sub".to_string(), serde_json::json!(user_sub).into());

        let rows: Vec<UserRow> = self
            .db
            .tenant_query(TenantQueryOperation::select(
                "user",
                Vec::new(),
                filters,
                None,
                Some(1),
            )?)
            .await
            .map_err(db_error)?;

        Ok(rows.into_iter().next().map(row_to_user))
    }

    async fn create_user(&self, user: &domain::User) -> Result<(), UserError> {
        let mut values: BTreeMap<String, SurrealFieldValue> = BTreeMap::new();
        values.insert("user_key".into(), serde_json::json!(user.id).into());
        values.insert("user_sub".into(), serde_json::json!(user.user_sub).into());
        values.insert(
            "display_name".into(),
            serde_json::json!(user.display_name).into(),
        );
        values.insert("email".into(), serde_json::json!(user.email).into());
        values.insert("created_at".into(), SurrealFieldValue::TimeNow);
        values.insert("last_login_at".into(), SurrealFieldValue::TimeNow);

        let _: Vec<serde_json::Value> = self
            .db
            .tenant_query(TenantQueryOperation::create("user", values)?)
            .await
            .map_err(db_error)?;
        Ok(())
    }

    async fn update_last_login(&self, user_sub: &str) -> Result<(), UserError> {
        let mut set = BTreeMap::new();
        set.insert("last_login_at".to_string(), SurrealFieldValue::TimeNow);
        let mut filters = BTreeMap::new();
        filters.insert("user_sub".to_string(), serde_json::json!(user_sub).into());

        let _: Vec<serde_json::Value> = self
            .db
            .tenant_query(TenantQueryOperation::update("user", set, filters)?)
            .await
            .map_err(db_error)?;
        Ok(())
    }
}

pub struct SurrealDbTenantRepository<P: SurrealDbPort> {
    db: P,
}

impl<P: SurrealDbPort> SurrealDbTenantRepository<P> {
    pub fn new(db: P) -> Self {
        Self { db }
    }
}

#[derive(Debug, Deserialize)]
struct TenantRow {
    #[serde(rename = "tenant_key")]
    id: String,
    name: String,
    created_at: String,
}

#[async_trait]
impl<P: SurrealDbPort> TenantRepository for SurrealDbTenantRepository<P> {
    async fn create_tenant(&self, name: &str) -> Result<String, UserError> {
        let tenant_id = generate_id();
        let mut values = BTreeMap::new();
        values.insert(
            "tenant_key".to_string(),
            serde_json::json!(tenant_id).into(),
        );
        values.insert("name".to_string(), serde_json::json!(name).into());

        let _: Vec<TenantRow> = self
            .db
            .tenant_query(TenantQueryOperation::create("tenant", values)?)
            .await
            .map_err(db_error)?;
        Ok(tenant_id)
    }

    async fn find_by_id(&self, tenant_id: &str) -> Result<Option<domain::Tenant>, UserError> {
        let mut filters = BTreeMap::new();
        filters.insert(
            "tenant_key".to_string(),
            serde_json::json!(tenant_id).into(),
        );

        let rows: Vec<TenantRow> = self
            .db
            .tenant_query(TenantQueryOperation::select(
                "tenant",
                Vec::new(),
                filters,
                None,
                Some(1),
            )?)
            .await
            .map_err(db_error)?;

        Ok(rows.into_iter().next().map(row_to_tenant))
    }
}

pub struct SurrealDbUserTenantRepository<P: SurrealDbPort> {
    db: P,
}

impl<P: SurrealDbPort> SurrealDbUserTenantRepository<P> {
    pub fn new(db: P) -> Self {
        Self { db }
    }
}

#[derive(Debug, Deserialize)]
struct BindingRow {
    id: Option<String>,
    user_sub: String,
    tenant_id: String,
    role: String,
    joined_at: String,
}

#[async_trait]
impl<P: SurrealDbPort> UserTenantRepository for SurrealDbUserTenantRepository<P> {
    async fn find_user_tenant(
        &self,
        user_sub: &str,
    ) -> Result<Option<UserTenantBinding>, UserError> {
        let mut filters = BTreeMap::new();
        filters.insert("user_sub".to_string(), serde_json::json!(user_sub).into());

        let rows: Vec<BindingRow> = self
            .db
            .tenant_query(TenantQueryOperation::select(
                "user_tenant",
                Vec::new(),
                filters,
                None,
                Some(1),
            )?)
            .await
            .map_err(db_error)?;

        Ok(rows.into_iter().next().map(row_to_binding))
    }

    async fn create_binding(
        &self,
        user_sub: &str,
        tenant_id: &str,
        role: &str,
    ) -> Result<UserTenantBinding, UserError> {
        let binding_id = generate_id();
        let mut values = BTreeMap::new();
        values.insert("id".to_string(), serde_json::json!(binding_id).into());
        values.insert("user_sub".to_string(), serde_json::json!(user_sub).into());
        values.insert("tenant_id".to_string(), serde_json::json!(tenant_id).into());
        values.insert("role".to_string(), serde_json::json!(role).into());
        values.insert("joined_at".to_string(), SurrealFieldValue::TimeNow);

        let rows: Vec<BindingRow> = self
            .db
            .tenant_query(TenantQueryOperation::create("user_tenant", values)?)
            .await
            .map_err(db_error)?;

        rows.into_iter()
            .next()
            .map(row_to_binding)
            .ok_or_else(|| UserError::Database("failed to create binding".to_string()))
    }
}

fn row_to_user(row: UserRow) -> domain::User {
    domain::User {
        id: row.id,
        user_sub: row.user_sub,
        display_name: row.display_name,
        email: row.email,
        created_at: parse_datetime(&row.created_at).unwrap_or_else(Utc::now),
        last_login_at: row.last_login_at.as_deref().and_then(parse_datetime),
    }
}

fn row_to_tenant(row: TenantRow) -> domain::Tenant {
    domain::Tenant {
        id: row.id,
        name: row.name,
        created_at: parse_datetime(&row.created_at).unwrap_or_else(Utc::now),
    }
}

fn row_to_binding(row: BindingRow) -> UserTenantBinding {
    UserTenantBinding {
        id: row.id.unwrap_or_else(generate_id),
        user_sub: row.user_sub,
        tenant_id: row.tenant_id,
        role: row.role,
        joined_at: parse_datetime(&row.joined_at).unwrap_or_else(Utc::now),
    }
}

fn parse_datetime(value: &str) -> Option<chrono::DateTime<Utc>> {
    chrono::DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}

fn db_error(error: impl std::fmt::Display) -> UserError {
    UserError::Database(error.to_string())
}

fn generate_id() -> String {
    let bytes: [u8; 16] = std::array::from_fn(|_| rand::random::<u8>());
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
