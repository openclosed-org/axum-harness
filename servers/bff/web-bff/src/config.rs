//! BFF 配置 — 环境变量 + figment 加载。

use security_runtime_policy::{RuntimeGuardViolation, RuntimeProfile, RuntimeSecurityPolicy};
use serde::{Deserialize, Serialize};

/// Web BFF 应用配置。
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Config {
    pub server_host: String,
    pub server_port: u16,
    pub cors_allowed_origins: Vec<String>,
    /// Authentication mode for protected API routes.
    /// - `jwt` (default): require `Authorization: Bearer <token>`.
    /// - `dev_headers`: allow local identity injection via `x-dev-user-sub` and optional
    ///   `x-dev-tenant-id` / `x-dev-user-roles` before falling back to Bearer auth.
    pub auth_mode: String,
    pub jwt_secret: String,
    /// OIDC issuer URL (e.g., "https://idp.example.com").
    /// When set, the middleware validates JWTs against OIDC discovery.
    /// Dev fallback: empty string -> uses `jwt_secret` for HS256 validation.
    pub oidc_issuer: String,
    /// Expected audience in JWT `aud` claim.
    /// Dev fallback: empty string → audience check skipped.
    pub oidc_audience: String,
    /// Optional explicit OIDC introspection URL. When omitted, discovery metadata may provide it.
    pub oidc_introspection_url: String,
    /// Optional OIDC introspection client id.
    /// When set together with `oidc_introspection_client_secret`, the BFF
    /// validates opaque access tokens through introspection instead of local JWKS verification.
    pub oidc_introspection_client_id: String,
    /// Optional OIDC introspection client secret.
    pub oidc_introspection_client_secret: String,
    /// Authorization provider. The template ships `openfga` as the local reference adapter.
    pub authz_provider: String,
    /// Authorization provider endpoint (e.g., "http://localhost:8081" for OpenFGA).
    /// When set, the BFF uses the configured real authorization adapter.
    /// Dev fallback: empty string → uses MockAuthzAdapter (allow-all).
    pub authz_endpoint: String,
    /// Authorization store id used by the real adapter.
    pub authz_store_id: String,
    /// Optional authorization model id.
    pub authz_model_id: String,
    /// Embedded Turso database URL (e.g., "file:path.db" or "memory").
    /// Used when turso_url is not set.
    pub database_url: Option<String>,
    /// Repository provider for all BFF-composed business services.
    /// Supported values: `turso`, `surrealdb`.
    pub store_provider: String,
    /// Remote Turso database URL (e.g., "libsql://your-db.turso.io").
    /// When set, the BFF connects to Turso cloud instead of embedded mode.
    pub turso_url: Option<String>,
    /// Turso authentication token for remote connections.
    pub turso_auth_token: Option<String>,
    /// External SurrealDB endpoint used when APP_STORE_PROVIDER=surrealdb.
    pub surrealdb_url: Option<String>,
    pub surrealdb_ns: String,
    pub surrealdb_db: String,
    pub surrealdb_user: String,
    pub surrealdb_pass: Option<String>,
    pub surrealdb_tenant_scope: String,
}

impl Config {
    /// 从环境变量加载配置（APP_ 前缀）。
    pub fn from_env() -> anyhow::Result<Self> {
        platform::load_config(Self::default(), "APP_", Some("APP_CONFIG_FILE")).map_err(Into::into)
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server_host: "0.0.0.0".to_string(),
            server_port: 3010,
            cors_allowed_origins: vec![],
            auth_mode: "jwt".to_string(),
            jwt_secret: "dev-secret-change-in-production".to_string(),
            oidc_issuer: String::new(),
            oidc_audience: String::new(),
            oidc_introspection_url: String::new(),
            oidc_introspection_client_id: String::new(),
            oidc_introspection_client_secret: String::new(),
            authz_provider: "openfga".to_string(),
            authz_endpoint: String::new(),
            authz_store_id: String::new(),
            authz_model_id: String::new(),
            database_url: None,
            store_provider: "turso".to_string(),
            turso_url: None,
            turso_auth_token: None,
            surrealdb_url: None,
            surrealdb_ns: "axh".to_string(),
            surrealdb_db: "main".to_string(),
            surrealdb_user: "root".to_string(),
            surrealdb_pass: None,
            surrealdb_tenant_scope: "platform".to_string(),
        }
    }
}

impl Config {
    pub fn allows_dev_headers(&self) -> bool {
        self.auth_mode.eq_ignore_ascii_case("dev_headers")
    }

    pub fn validate_runtime(&self) -> anyhow::Result<()> {
        self.validate_runtime_profile(RuntimeProfile::from_env())
            .map_err(anyhow::Error::from)
    }

    fn dev_secret() -> &'static str {
        "dev-secret-change-in-production"
    }
}

impl RuntimeSecurityPolicy for Config {
    fn validate_runtime_profile(
        &self,
        profile: RuntimeProfile,
    ) -> Result<(), RuntimeGuardViolation> {
        if !profile.is_production() {
            return Ok(());
        }

        if self.allows_dev_headers() {
            return Err(RuntimeGuardViolation::new(
                "APP_AUTH_MODE",
                "APP_AUTH_MODE=dev_headers is not allowed in production",
            ));
        }

        if self.oidc_issuer.trim().is_empty() && self.jwt_secret == Self::dev_secret() {
            return Err(RuntimeGuardViolation::new(
                "APP_OIDC_ISSUER",
                "production requires APP_OIDC_ISSUER or a non-default APP_JWT_SECRET",
            ));
        }

        if self.authz_endpoint.trim().is_empty() {
            return Err(RuntimeGuardViolation::new(
                "APP_AUTHZ_ENDPOINT",
                "production requires APP_AUTHZ_ENDPOINT",
            ));
        }

        if self.store_provider.eq_ignore_ascii_case("surrealdb") {
            if self
                .surrealdb_url
                .as_deref()
                .unwrap_or_default()
                .trim()
                .is_empty()
            {
                return Err(RuntimeGuardViolation::new(
                    "APP_SURREALDB_URL",
                    "APP_STORE_PROVIDER=surrealdb requires APP_SURREALDB_URL",
                ));
            }
            if self
                .surrealdb_pass
                .as_deref()
                .unwrap_or_default()
                .trim()
                .is_empty()
            {
                return Err(RuntimeGuardViolation::new(
                    "APP_SURREALDB_PASS",
                    "APP_STORE_PROVIDER=surrealdb requires APP_SURREALDB_PASS",
                ));
            }
        }

        if self.cors_allowed_origins.is_empty() {
            return Err(RuntimeGuardViolation::new(
                "APP_CORS_ALLOWED_ORIGINS",
                "production requires APP_CORS_ALLOWED_ORIGINS allowlist",
            ));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn production_ready_config() -> Config {
        Config {
            jwt_secret: "safe-production-secret".to_string(),
            authz_endpoint: "http://localhost:8081".to_string(),
            cors_allowed_origins: vec!["https://example.com".to_string()],
            ..Config::default()
        }
    }

    #[test]
    fn production_rejects_default_jwt_secret_without_oidc_issuer() {
        let config = production_ready_config();
        let config = Config {
            jwt_secret: Config::dev_secret().to_string(),
            oidc_issuer: String::new(),
            ..config
        };

        let error = config
            .validate_runtime_profile(RuntimeProfile::Production)
            .unwrap_err()
            .to_string();
        assert!(error.contains("APP_OIDC_ISSUER"));
    }

    #[test]
    fn production_rejects_missing_authz_endpoint() {
        let config = Config {
            authz_endpoint: String::new(),
            ..production_ready_config()
        };

        let error = config
            .validate_runtime_profile(RuntimeProfile::Production)
            .unwrap_err()
            .to_string();
        assert!(error.contains("APP_AUTHZ_ENDPOINT"));
    }

    #[test]
    fn production_rejects_permissive_cors_default() {
        let config = Config {
            cors_allowed_origins: Vec::new(),
            ..production_ready_config()
        };

        let error = config
            .validate_runtime_profile(RuntimeProfile::Production)
            .unwrap_err()
            .to_string();
        assert!(error.contains("APP_CORS_ALLOWED_ORIGINS"));
    }

    #[test]
    fn production_rejects_dev_headers() {
        let config = Config {
            auth_mode: "dev_headers".to_string(),
            ..production_ready_config()
        };

        let error = config
            .validate_runtime_profile(RuntimeProfile::Production)
            .unwrap_err()
            .to_string();
        assert!(error.contains("APP_AUTH_MODE"));
    }

    #[test]
    fn non_production_allows_dev_headers() {
        let config = Config {
            auth_mode: "dev_headers".to_string(),
            ..Config::default()
        };

        config
            .validate_runtime_profile(RuntimeProfile::Development)
            .unwrap();
    }
}
