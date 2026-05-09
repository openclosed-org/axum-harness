//! Infrastructure layer — LibSQL implementations of repository ports.

pub mod surrealdb_adapter;

pub use surrealdb_adapter::{
    SurrealDbTenantRepository, SurrealDbUserRepository, SurrealDbUserTenantRepository,
};

use async_trait::async_trait;
use chrono::Utc;
use serde::Deserialize;

use crate::domain;
use crate::domain::UserTenantBinding;
use crate::domain::error::UserError;
use crate::ports::{TenantRepository, UserRepository, UserTenantRepository};
use data::ports::lib_sql::LibSqlPort;

/// LibSQL implementation of UserRepository.
pub struct LibSqlUserRepository<P> {
    db: P,
}

impl<P: LibSqlPort> LibSqlUserRepository<P> {
    pub fn new(db: P) -> Self {
        Self { db }
    }
}

#[derive(Debug, Deserialize)]
struct UserRow {
    id: String,
    user_sub: String,
    display_name: String,
    email: Option<String>,
    created_at: String,
    last_login_at: Option<String>,
}

#[async_trait]
impl<P: LibSqlPort> UserRepository for LibSqlUserRepository<P> {
    async fn find_by_sub(&self, user_sub: &str) -> Result<Option<domain::User>, UserError> {
        let rows: Vec<UserRow> = self
            .db
            .query(
                "SELECT id, user_sub, display_name, email, created_at, last_login_at \
                 FROM user WHERE user_sub = ? LIMIT 1",
                vec![user_sub.to_string()],
            )
            .await
            .map_err(|e| UserError::Database(format!("query failed: {}", e)))?;

        let row = match rows.first() {
            Some(r) => r,
            None => return Ok(None),
        };

        Ok(Some(domain::User {
            id: row.id.clone(),
            user_sub: row.user_sub.clone(),
            display_name: row.display_name.clone(),
            email: row.email.clone(),
            created_at: chrono::DateTime::parse_from_rfc3339(&row.created_at)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            last_login_at: row.last_login_at.as_ref().and_then(|s| {
                chrono::DateTime::parse_from_rfc3339(s)
                    .ok()
                    .map(|dt| dt.with_timezone(&Utc))
            }),
        }))
    }

    async fn create_user(&self, user: &domain::User) -> Result<(), UserError> {
        self.db
            .execute(
                "INSERT INTO user (id, user_sub, display_name, email, created_at, last_login_at) \
                 VALUES (?, ?, ?, ?, ?, ?)",
                vec![
                    user.id.clone(),
                    user.user_sub.clone(),
                    user.display_name.clone(),
                    user.email.clone().unwrap_or_default(),
                    user.created_at.to_rfc3339(),
                    user.last_login_at
                        .as_ref()
                        .map(|dt| dt.to_rfc3339())
                        .unwrap_or_default(),
                ],
            )
            .await
            .map_err(|e| UserError::Database(format!("insert failed: {}", e)))?;

        Ok(())
    }

    async fn update_last_login(&self, user_sub: &str) -> Result<(), UserError> {
        let now = Utc::now().to_rfc3339();
        self.db
            .execute(
                "UPDATE user SET last_login_at = ? WHERE user_sub = ?",
                vec![now, user_sub.to_string()],
            )
            .await
            .map_err(|e| UserError::Database(format!("update failed: {}", e)))?;

        Ok(())
    }
}

/// LibSQL implementation of TenantRepository.
pub struct LibSqlTenantRepository<P> {
    db: P,
}

impl<P: LibSqlPort> LibSqlTenantRepository<P> {
    pub fn new(db: P) -> Self {
        Self { db }
    }
}

#[derive(Debug, Deserialize)]
struct TenantRow {
    id: String,
    name: String,
    created_at: String,
}

#[derive(Debug, Deserialize)]
struct CreatedTenantRow {
    id: String,
}

#[async_trait]
impl<P: LibSqlPort> TenantRepository for LibSqlTenantRepository<P> {
    async fn create_tenant(&self, name: &str) -> Result<String, UserError> {
        let rows: Vec<CreatedTenantRow> = self
            .db
            .query(
                "INSERT INTO tenant (id, name) VALUES (lower(hex(randomblob(16))), ?) RETURNING id",
                vec![name.to_string()],
            )
            .await
            .map_err(|e| UserError::Database(format!("insert failed: {}", e)))?;

        let row = rows
            .first()
            .ok_or_else(|| UserError::Database("failed to create tenant".to_string()))?;

        Ok(row.id.clone())
    }

    async fn find_by_id(&self, tenant_id: &str) -> Result<Option<domain::Tenant>, UserError> {
        let rows: Vec<TenantRow> = self
            .db
            .query(
                "SELECT id, name, created_at FROM tenant WHERE id = ? LIMIT 1",
                vec![tenant_id.to_string()],
            )
            .await
            .map_err(|e| UserError::Database(format!("query failed: {}", e)))?;

        let row = match rows.first() {
            Some(r) => r,
            None => return Ok(None),
        };

        Ok(Some(domain::Tenant {
            id: row.id.clone(),
            name: row.name.clone(),
            created_at: chrono::DateTime::parse_from_rfc3339(&row.created_at)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
        }))
    }
}

/// LibSQL implementation of UserTenantRepository.
pub struct LibSqlUserTenantRepository<P> {
    db: P,
}

impl<P: LibSqlPort> LibSqlUserTenantRepository<P> {
    pub fn new(db: P) -> Self {
        Self { db }
    }
}

#[derive(Debug, Deserialize)]
struct BindingRow {
    id: String,
    user_sub: String,
    tenant_id: String,
    role: String,
    joined_at: String,
}

#[async_trait]
impl<P: LibSqlPort> UserTenantRepository for LibSqlUserTenantRepository<P> {
    async fn find_user_tenant(
        &self,
        user_sub: &str,
    ) -> Result<Option<UserTenantBinding>, UserError> {
        let rows: Vec<BindingRow> = self
            .db
            .query(
                "SELECT id, user_sub, tenant_id, role, joined_at \
                 FROM user_tenant WHERE user_sub = ? LIMIT 1",
                vec![user_sub.to_string()],
            )
            .await
            .map_err(|e| UserError::Database(format!("query failed: {}", e)))?;

        let row = match rows.first() {
            Some(r) => r,
            None => return Ok(None),
        };

        Ok(Some(UserTenantBinding {
            id: row.id.clone(),
            user_sub: row.user_sub.clone(),
            tenant_id: row.tenant_id.clone(),
            role: row.role.clone(),
            joined_at: parse_joined_at(&row.joined_at).unwrap_or_else(Utc::now),
        }))
    }

    async fn create_binding(
        &self,
        user_sub: &str,
        tenant_id: &str,
        role: &str,
    ) -> Result<UserTenantBinding, UserError> {
        self.db
            .execute(
                "INSERT INTO user_tenant (id, user_sub, tenant_id, role, joined_at) \
                 VALUES (lower(hex(randomblob(16))), ?, ?, ?, datetime('now'))",
                vec![
                    user_sub.to_string(),
                    tenant_id.to_string(),
                    role.to_string(),
                ],
            )
            .await
            .map_err(|e| UserError::Database(format!("insert failed: {}", e)))?;

        // Fetch the created binding
        let binding = self
            .find_user_tenant(user_sub)
            .await?
            .ok_or_else(|| UserError::Database("failed to fetch created binding".to_string()))?;

        Ok(binding)
    }
}

fn parse_joined_at(value: &str) -> Option<chrono::DateTime<Utc>> {
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(value) {
        return Some(dt.with_timezone(&Utc));
    }

    chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S")
        .ok()
        .map(|dt| dt.and_utc())
}
