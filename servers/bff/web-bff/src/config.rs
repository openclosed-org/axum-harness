//! BFF 配置 — 环境变量 + figment 加载。

use security_runtime_policy::{RuntimeGuardViolation, RuntimeProfile, RuntimeSecurityPolicy};
use serde::{Deserialize, Deserializer, Serialize};

/// Web BFF 应用配置。
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Config {
    pub server_host: String,
    pub server_port: u16,
    #[serde(deserialize_with = "deserialize_string_list")]
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
    /// Commercial guard mode for Phase 1 seams.
    /// Supported values: `disabled`, `local_mock`, `local_real`.
    pub commercial_mode: String,
    /// Capability key checked before tenant-scoped counter writes when commercial mode is enabled.
    pub counter_paid_capability: String,
    /// Static allow-list used by `APP_COMMERCIAL_MODE=local_mock`.
    #[serde(deserialize_with = "deserialize_string_list")]
    pub commercial_mock_allowed_capabilities: Vec<String>,
    /// Billing provider adapter. Supported values: `disabled`, `creem`.
    pub billing_provider: String,
    /// Creem environment. Supported values: `test`, `live`.
    pub creem_env: String,
    /// Creem API key. Used only by server-side checkout creation.
    pub creem_api_key: String,
    /// Creem webhook HMAC secret.
    pub creem_webhook_secret: String,
    /// Creem product mapped to the counter paid capability.
    pub creem_product_counter_pro: String,
    /// Public base URL used to build Creem checkout success URLs.
    pub public_base_url: String,
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
            commercial_mode: "disabled".to_string(),
            counter_paid_capability: "counter.write".to_string(),
            commercial_mock_allowed_capabilities: vec!["counter.write".to_string()],
            billing_provider: "disabled".to_string(),
            creem_env: "test".to_string(),
            creem_api_key: String::new(),
            creem_webhook_secret: String::new(),
            creem_product_counter_pro: String::new(),
            public_base_url: "http://localhost:3010".to_string(),
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

    pub fn commercial_capability_state(
        &self,
    ) -> Result<CommercialCapabilityState, RuntimeGuardViolation> {
        CommercialCapabilityState::parse(&self.commercial_mode)
    }

    pub fn implemented_commercial_mode(
        &self,
    ) -> Result<ImplementedCommercialMode, RuntimeGuardViolation> {
        self.commercial_capability_state()?.implemented_mode()
    }

    fn dev_secret() -> &'static str {
        "dev-secret-change-in-production"
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommercialCapabilityState {
    Disabled,
    LocalMock,
    LocalReal,
    ExternalSingleNode,
    ExternalDistributed,
}

impl CommercialCapabilityState {
    fn parse(value: &str) -> Result<Self, RuntimeGuardViolation> {
        match value {
            value if value.eq_ignore_ascii_case("disabled") => Ok(Self::Disabled),
            value if value.eq_ignore_ascii_case("local_mock") => Ok(Self::LocalMock),
            value if value.eq_ignore_ascii_case("local_real") => Ok(Self::LocalReal),
            value if value.eq_ignore_ascii_case("external_single_node") => {
                Ok(Self::ExternalSingleNode)
            }
            value if value.eq_ignore_ascii_case("external_distributed") => {
                Ok(Self::ExternalDistributed)
            }
            _ => Err(RuntimeGuardViolation::new(
                "APP_COMMERCIAL_MODE",
                "APP_COMMERCIAL_MODE must be one of disabled, local_mock, local_real, external_single_node, or external_distributed",
            )),
        }
    }

    fn implemented_mode(self) -> Result<ImplementedCommercialMode, RuntimeGuardViolation> {
        match self {
            Self::Disabled => Ok(ImplementedCommercialMode::Disabled),
            Self::LocalMock => Ok(ImplementedCommercialMode::LocalMock),
            Self::LocalReal => Ok(ImplementedCommercialMode::LocalReal),
            Self::ExternalSingleNode | Self::ExternalDistributed => {
                Err(RuntimeGuardViolation::new(
                    "APP_COMMERCIAL_MODE",
                    "APP_COMMERCIAL_MODE external states are canonical but not implemented by web-bff commercial composition yet",
                ))
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImplementedCommercialMode {
    Disabled,
    LocalMock,
    LocalReal,
}

impl RuntimeSecurityPolicy for Config {
    fn validate_runtime_profile(
        &self,
        profile: RuntimeProfile,
    ) -> Result<(), RuntimeGuardViolation> {
        let commercial_mode = self.implemented_commercial_mode()?;
        self.validate_billing_provider()?;

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

        if commercial_mode == ImplementedCommercialMode::LocalMock {
            return Err(RuntimeGuardViolation::new(
                "APP_COMMERCIAL_MODE",
                "APP_COMMERCIAL_MODE=local_mock is not allowed in production",
            ));
        }

        if !matches!(
            commercial_mode,
            ImplementedCommercialMode::Disabled | ImplementedCommercialMode::LocalReal
        ) {
            return Err(RuntimeGuardViolation::new(
                "APP_COMMERCIAL_MODE",
                "production commercial mode must use disabled or local_real",
            ));
        }

        if self.billing_provider.eq_ignore_ascii_case("creem") {
            if commercial_mode != ImplementedCommercialMode::LocalReal {
                return Err(RuntimeGuardViolation::new(
                    "APP_COMMERCIAL_MODE",
                    "production Creem billing requires APP_COMMERCIAL_MODE=local_real",
                ));
            }

            if !self.creem_env.eq_ignore_ascii_case("live") {
                return Err(RuntimeGuardViolation::new(
                    "APP_CREEM_ENV",
                    "production Creem billing requires APP_CREEM_ENV=live",
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

impl Config {
    fn validate_billing_provider(&self) -> Result<(), RuntimeGuardViolation> {
        if self.billing_provider.eq_ignore_ascii_case("creem") {
            if !self.creem_env.eq_ignore_ascii_case("test")
                && !self.creem_env.eq_ignore_ascii_case("live")
            {
                return Err(RuntimeGuardViolation::new(
                    "APP_CREEM_ENV",
                    "APP_CREEM_ENV must be test or live",
                ));
            }

            if self.creem_api_key.trim().is_empty() {
                return Err(RuntimeGuardViolation::new(
                    "APP_CREEM_API_KEY",
                    "APP_BILLING_PROVIDER=creem requires APP_CREEM_API_KEY",
                ));
            }

            if self.creem_webhook_secret.trim().is_empty() {
                return Err(RuntimeGuardViolation::new(
                    "APP_CREEM_WEBHOOK_SECRET",
                    "APP_BILLING_PROVIDER=creem requires APP_CREEM_WEBHOOK_SECRET",
                ));
            }

            if self.creem_product_counter_pro.trim().is_empty() {
                return Err(RuntimeGuardViolation::new(
                    "APP_CREEM_PRODUCT_COUNTER_PRO",
                    "APP_BILLING_PROVIDER=creem requires APP_CREEM_PRODUCT_COUNTER_PRO",
                ));
            }

            if self.public_base_url.trim().is_empty() {
                return Err(RuntimeGuardViolation::new(
                    "APP_PUBLIC_BASE_URL",
                    "APP_BILLING_PROVIDER=creem requires APP_PUBLIC_BASE_URL",
                ));
            }

            return Ok(());
        }

        if !self.billing_provider.eq_ignore_ascii_case("disabled") {
            return Err(RuntimeGuardViolation::new(
                "APP_BILLING_PROVIDER",
                "APP_BILLING_PROVIDER must be disabled or creem",
            ));
        }

        Ok(())
    }
}

fn deserialize_string_list<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum StringList {
        One(String),
        Many(Vec<String>),
    }

    let list = StringList::deserialize(deserializer)?;
    let values = match list {
        StringList::One(value) => value
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .collect(),
        StringList::Many(values) => values,
    };
    Ok(values)
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
    fn production_rejects_local_mock_commercial_mode() {
        let config = Config {
            commercial_mode: "local_mock".to_string(),
            ..production_ready_config()
        };

        let error = config
            .validate_runtime_profile(RuntimeProfile::Production)
            .unwrap_err()
            .to_string();
        assert!(error.contains("APP_COMMERCIAL_MODE"));
    }

    #[test]
    fn rejects_local_dev_commercial_mode() {
        let config = Config {
            commercial_mode: "local_dev".to_string(),
            ..Config::default()
        };

        let error = config
            .validate_runtime_profile(RuntimeProfile::Development)
            .unwrap_err()
            .to_string();
        assert!(error.contains("APP_COMMERCIAL_MODE"));
    }

    #[test]
    fn rejects_unimplemented_external_commercial_state() {
        let config = Config {
            commercial_mode: "external_single_node".to_string(),
            ..Config::default()
        };

        let error = config
            .validate_runtime_profile(RuntimeProfile::Development)
            .unwrap_err()
            .to_string();
        assert!(error.contains("not implemented"));
    }

    #[test]
    fn parses_canonical_commercial_capability_states() {
        let cases = [
            ("disabled", CommercialCapabilityState::Disabled),
            ("local_mock", CommercialCapabilityState::LocalMock),
            ("local_real", CommercialCapabilityState::LocalReal),
            (
                "external_single_node",
                CommercialCapabilityState::ExternalSingleNode,
            ),
            (
                "external_distributed",
                CommercialCapabilityState::ExternalDistributed,
            ),
        ];

        for (value, expected) in cases {
            let config = Config {
                commercial_mode: value.to_string(),
                ..Config::default()
            };
            assert_eq!(config.commercial_capability_state().unwrap(), expected);
        }
    }

    #[test]
    fn production_allows_local_real_commercial_mode() {
        let config = Config {
            commercial_mode: "local_real".to_string(),
            ..production_ready_config()
        };

        config
            .validate_runtime_profile(RuntimeProfile::Production)
            .unwrap();
    }

    #[test]
    fn non_production_creem_requires_provider_settings() {
        let config = Config {
            billing_provider: "creem".to_string(),
            ..Config::default()
        };

        let error = config
            .validate_runtime_profile(RuntimeProfile::Development)
            .unwrap_err()
            .to_string();
        assert!(error.contains("APP_CREEM_API_KEY"));
    }

    #[test]
    fn production_creem_requires_live_env_and_local_real() {
        let config = Config {
            commercial_mode: "disabled".to_string(),
            billing_provider: "creem".to_string(),
            creem_env: "test".to_string(),
            creem_api_key: "test-key".to_string(),
            creem_webhook_secret: "test-secret".to_string(),
            creem_product_counter_pro: "prod_test".to_string(),
            public_base_url: "https://example.com".to_string(),
            ..production_ready_config()
        };

        let error = config
            .validate_runtime_profile(RuntimeProfile::Production)
            .unwrap_err()
            .to_string();
        assert!(error.contains("APP_COMMERCIAL_MODE"));

        let config = Config {
            commercial_mode: "local_real".to_string(),
            ..config
        };
        let error = config
            .validate_runtime_profile(RuntimeProfile::Production)
            .unwrap_err()
            .to_string();
        assert!(error.contains("APP_CREEM_ENV"));
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

    #[test]
    fn parses_comma_separated_cors_origins_from_env_shape() {
        #[derive(Deserialize)]
        struct CorsOnly {
            #[serde(deserialize_with = "deserialize_string_list")]
            cors_allowed_origins: Vec<String>,
        }

        let config: CorsOnly = serde_json::from_value(serde_json::json!({
            "cors_allowed_origins": "http://localhost:5173,http://localhost:3000"
        }))
        .unwrap();

        assert_eq!(
            config.cors_allowed_origins,
            vec!["http://localhost:5173", "http://localhost:3000"]
        );
    }
}
