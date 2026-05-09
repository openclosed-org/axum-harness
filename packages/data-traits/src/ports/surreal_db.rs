use async_trait::async_trait;
use serde::de::DeserializeOwned;
use std::collections::BTreeMap;
use std::fmt;

/// Error type for SurrealDB operations.
pub type SurrealError = Box<dyn std::error::Error + Send + Sync>;

/// Explicit capability token for raw SurrealQL.
///
/// Tenant-scoped code must use [`TenantQueryOperation`]. Raw query execution is
/// reserved for admin paths such as migrations, backup/restore verification, and
/// one-off operator diagnostics.
#[derive(Debug, Clone, Copy)]
pub struct SurrealAdminMarker {
    _private: (),
}

impl SurrealAdminMarker {
    /// Construct the explicit unsafe/admin marker required for raw SurrealQL.
    pub const fn unsafe_admin() -> Self {
        Self { _private: () }
    }
}

/// A validated SurrealDB identifier used for table, field, and edge names.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SurrealIdent(String);

impl SurrealIdent {
    pub fn new(value: impl Into<String>) -> Result<Self, SurrealQueryBuildError> {
        let value = value.into();
        if value.is_empty()
            || !value
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            return Err(SurrealQueryBuildError::InvalidIdentifier(value));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SurrealIdent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Typed value allowed in tenant-safe SurrealQL operations.
#[derive(Debug, Clone, PartialEq)]
pub enum SurrealFieldValue {
    Json(serde_json::Value),
    TimeNow,
}

impl From<serde_json::Value> for SurrealFieldValue {
    fn from(value: serde_json::Value) -> Self {
        Self::Json(value)
    }
}

/// Sort direction for typed select operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurrealOrderDirection {
    Asc,
    Desc,
}

impl SurrealOrderDirection {
    fn as_sql(self) -> &'static str {
        match self {
            Self::Asc => "ASC",
            Self::Desc => "DESC",
        }
    }
}

/// Typed tenant-scoped SurrealDB operations.
///
/// This deliberately supports a narrow subset first. Tenant code cannot pass raw
/// SurrealQL and cannot supply its own `tenant_scope`; the adapter injects the
/// authenticated scope at the final boundary. Service tables can still use a
/// business `tenant_id` field, matching the common Postgres pattern where the
/// tenant registry is scoped separately from tenant-owned records.
#[derive(Debug, Clone, PartialEq)]
pub enum TenantQueryOperation {
    Create {
        table: SurrealIdent,
        content: BTreeMap<SurrealIdent, SurrealFieldValue>,
    },
    Select {
        table: SurrealIdent,
        fields: Vec<SurrealIdent>,
        filters: BTreeMap<SurrealIdent, SurrealFieldValue>,
        order_by: Option<(SurrealIdent, SurrealOrderDirection)>,
        limit: Option<u32>,
    },
    Update {
        table: SurrealIdent,
        set: BTreeMap<SurrealIdent, SurrealFieldValue>,
        filters: BTreeMap<SurrealIdent, SurrealFieldValue>,
    },
    Delete {
        table: SurrealIdent,
        filters: BTreeMap<SurrealIdent, SurrealFieldValue>,
    },
    GraphTraverse {
        from_table: SurrealIdent,
        from_id: SurrealFieldValue,
        edge_table: SurrealIdent,
        target_table: SurrealIdent,
    },
    LiveTable {
        table: SurrealIdent,
        filters: BTreeMap<SurrealIdent, SurrealFieldValue>,
    },
}

impl TenantQueryOperation {
    pub fn create(
        table: impl Into<String>,
        content: BTreeMap<String, SurrealFieldValue>,
    ) -> Result<Self, SurrealQueryBuildError> {
        reject_reserved_scope_key(content.keys())?;
        Ok(Self::Create {
            table: SurrealIdent::new(table)?,
            content: validate_map(content)?,
        })
    }

    pub fn select(
        table: impl Into<String>,
        fields: Vec<String>,
        filters: BTreeMap<String, SurrealFieldValue>,
        order_by: Option<(String, SurrealOrderDirection)>,
        limit: Option<u32>,
    ) -> Result<Self, SurrealQueryBuildError> {
        reject_reserved_scope_key(filters.keys())?;
        Ok(Self::Select {
            table: SurrealIdent::new(table)?,
            fields: validate_idents(fields)?,
            filters: validate_map(filters)?,
            order_by: order_by
                .map(|(field, direction)| Ok((SurrealIdent::new(field)?, direction)))
                .transpose()?,
            limit,
        })
    }

    pub fn delete(
        table: impl Into<String>,
        filters: BTreeMap<String, SurrealFieldValue>,
    ) -> Result<Self, SurrealQueryBuildError> {
        reject_reserved_scope_key(filters.keys())?;
        Ok(Self::Delete {
            table: SurrealIdent::new(table)?,
            filters: validate_map(filters)?,
        })
    }

    pub fn update(
        table: impl Into<String>,
        set: BTreeMap<String, SurrealFieldValue>,
        filters: BTreeMap<String, SurrealFieldValue>,
    ) -> Result<Self, SurrealQueryBuildError> {
        reject_reserved_scope_key(set.keys())?;
        reject_reserved_scope_key(filters.keys())?;
        Ok(Self::Update {
            table: SurrealIdent::new(table)?,
            set: validate_map(set)?,
            filters: validate_map(filters)?,
        })
    }

    pub fn graph_traverse(
        from_table: impl Into<String>,
        from_id: impl Into<SurrealFieldValue>,
        edge_table: impl Into<String>,
        target_table: impl Into<String>,
    ) -> Result<Self, SurrealQueryBuildError> {
        Ok(Self::GraphTraverse {
            from_table: SurrealIdent::new(from_table)?,
            from_id: from_id.into(),
            edge_table: SurrealIdent::new(edge_table)?,
            target_table: SurrealIdent::new(target_table)?,
        })
    }

    pub fn live_table(
        table: impl Into<String>,
        filters: BTreeMap<String, SurrealFieldValue>,
    ) -> Result<Self, SurrealQueryBuildError> {
        reject_reserved_scope_key(filters.keys())?;
        Ok(Self::LiveTable {
            table: SurrealIdent::new(table)?,
            filters: validate_map(filters)?,
        })
    }

    pub fn to_surrealql(&self, tenant_scope: &str) -> Result<String, SurrealQueryBuildError> {
        let scope = sql_json(&serde_json::Value::String(tenant_scope.to_string()))?;
        match self {
            Self::Create { table, content } => {
                let mut fields = render_assignments(content)?;
                fields.push(format!("tenant_scope: {scope}"));
                Ok(format!(
                    "CREATE {table} CONTENT {{ {} }} RETURN AFTER",
                    fields.join(", ")
                ))
            }
            Self::Select {
                table,
                fields,
                filters,
                order_by,
                limit,
            } => {
                let field_list = if fields.is_empty() {
                    "*".to_string()
                } else {
                    fields
                        .iter()
                        .map(SurrealIdent::as_str)
                        .collect::<Vec<_>>()
                        .join(", ")
                };
                let mut sql = format!(
                    "SELECT {field_list} FROM {table} WHERE {}",
                    render_where(filters, &scope)?
                );
                if let Some((field, direction)) = order_by {
                    sql.push_str(&format!(" ORDER BY {field} {}", direction.as_sql()));
                }
                if let Some(limit) = limit {
                    sql.push_str(&format!(" LIMIT {limit}"));
                }
                Ok(sql)
            }
            Self::Delete { table, filters } => Ok(format!(
                "DELETE {table} WHERE {}",
                render_where(filters, &scope)?
            )),
            Self::Update {
                table,
                set,
                filters,
            } => Ok(format!(
                "UPDATE {table} SET {} WHERE {} RETURN AFTER",
                render_set(set)?,
                render_where(filters, &scope)?
            )),
            Self::GraphTraverse {
                from_table,
                from_id,
                edge_table,
                target_table,
            } => Ok(format!(
                "SELECT ->{edge_table}[WHERE tenant_scope = {scope}]->{target_table}[WHERE tenant_scope = {scope}] FROM type::thing(\"{from_table}\", {}) WHERE tenant_scope = {scope}",
                render_value(from_id)?
            )),
            Self::LiveTable { table, filters } => Ok(format!(
                "LIVE SELECT * FROM {table} WHERE {}",
                render_where(filters, &scope)?
            )),
        }
    }
}

/// Build-time validation errors for tenant-safe SurrealDB operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SurrealQueryBuildError {
    InvalidIdentifier(String),
    ReservedTenantField,
    Serialization(String),
}

impl fmt::Display for SurrealQueryBuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidIdentifier(value) => write!(f, "invalid SurrealDB identifier: {value}"),
            Self::ReservedTenantField => f.write_str("tenant_scope is adapter-owned"),
            Self::Serialization(value) => write!(f, "failed to serialize SurrealDB value: {value}"),
        }
    }
}

impl std::error::Error for SurrealQueryBuildError {}

fn validate_idents(values: Vec<String>) -> Result<Vec<SurrealIdent>, SurrealQueryBuildError> {
    values.into_iter().map(SurrealIdent::new).collect()
}

fn validate_map(
    values: BTreeMap<String, SurrealFieldValue>,
) -> Result<BTreeMap<SurrealIdent, SurrealFieldValue>, SurrealQueryBuildError> {
    values
        .into_iter()
        .map(|(key, value)| Ok((SurrealIdent::new(key)?, value)))
        .collect()
}

fn reject_reserved_scope_key<'a>(
    keys: impl Iterator<Item = &'a String>,
) -> Result<(), SurrealQueryBuildError> {
    if keys.into_iter().any(|key| key == "tenant_scope") {
        return Err(SurrealQueryBuildError::ReservedTenantField);
    }
    Ok(())
}

fn render_where(
    filters: &BTreeMap<SurrealIdent, SurrealFieldValue>,
    tenant: &str,
) -> Result<String, SurrealQueryBuildError> {
    let mut parts = vec![format!("tenant_scope = {tenant}")];
    for (field, value) in filters {
        parts.push(format!("{field} = {}", render_value(value)?));
    }
    Ok(parts.join(" AND "))
}

fn render_assignments(
    values: &BTreeMap<SurrealIdent, SurrealFieldValue>,
) -> Result<Vec<String>, SurrealQueryBuildError> {
    values
        .iter()
        .map(|(field, value)| Ok(format!("{field}: {}", render_value(value)?)))
        .collect()
}

fn render_set(
    values: &BTreeMap<SurrealIdent, SurrealFieldValue>,
) -> Result<String, SurrealQueryBuildError> {
    Ok(values
        .iter()
        .map(|(field, value)| Ok(format!("{field} = {}", render_value(value)?)))
        .collect::<Result<Vec<_>, SurrealQueryBuildError>>()?
        .join(", "))
}

fn render_value(value: &SurrealFieldValue) -> Result<String, SurrealQueryBuildError> {
    match value {
        SurrealFieldValue::Json(value) => sql_json(value),
        SurrealFieldValue::TimeNow => Ok("time::now()".to_string()),
    }
}

fn sql_json(value: &serde_json::Value) -> Result<String, SurrealQueryBuildError> {
    serde_json::to_string(value)
        .map_err(|err| SurrealQueryBuildError::Serialization(err.to_string()))
}

/// SurrealDB port — abstracts server-side SurrealDB access.
///
/// Implementations live in data-adapters crates.
/// SurrealDB uses SurrealQL (NOT standard SQL), hence a separate trait from LibSqlPort.
#[async_trait]
pub trait SurrealDbPort: Send + Sync {
    /// Verify the database connection is alive.
    async fn health_check(&self) -> Result<(), SurrealError>;
    /// Execute a typed tenant-scoped operation returning deserialized records.
    async fn tenant_query<T: DeserializeOwned + Send + Sync>(
        &self,
        operation: TenantQueryOperation,
    ) -> Result<Vec<T>, SurrealError>;
    /// Execute raw SurrealQL only when an explicit unsafe/admin marker is supplied.
    async fn unsafe_admin_query<T: DeserializeOwned + Send + Sync>(
        &self,
        marker: SurrealAdminMarker,
        sql: &str,
    ) -> Result<Vec<T>, SurrealError>;
}
