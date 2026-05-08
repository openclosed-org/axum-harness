use jsonwebtoken::{
    Algorithm, DecodingKey, Header, Validation, decode, decode_header, jwk::JwkSet,
};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

#[derive(Debug, Clone)]
pub struct OidcVerifierConfig {
    pub issuer: String,
    pub audience: String,
    pub introspection_url: String,
    pub introspection_client_id: String,
    pub introspection_client_secret: String,
}

#[derive(Debug, Clone)]
pub struct VerifiedIdentity {
    pub sub: String,
    pub tenant_id: Option<String>,
    pub roles: Vec<String>,
    pub email: Option<String>,
    pub issuer: Option<String>,
    pub audience: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize)]
struct IdTokenClaims {
    sub: String,
    exp: Option<i64>,
    tenant_id: Option<String>,
    roles: Option<Vec<String>>,
    email: Option<String>,
    aud: Option<serde_json::Value>,
    iss: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct OpenIdConfiguration {
    jwks_uri: String,
    introspection_endpoint: Option<String>,
}

#[derive(Debug, Deserialize)]
struct IntrospectionResponse {
    active: bool,
    sub: Option<String>,
    username: Option<String>,
    aud: Option<serde_json::Value>,
    iss: Option<String>,
}

#[derive(Clone)]
pub struct OidcVerifier {
    config: OidcVerifierConfig,
    client: reqwest::Client,
    discovery: Arc<RwLock<Option<OpenIdConfiguration>>>,
    jwks: Arc<RwLock<Option<JwkSet>>>,
}

impl OidcVerifier {
    pub fn new(config: OidcVerifierConfig, client: reqwest::Client) -> Self {
        Self {
            config,
            client,
            discovery: Arc::new(RwLock::new(None)),
            jwks: Arc::new(RwLock::new(None)),
        }
    }

    pub async fn verify(&self, token: &str) -> Result<VerifiedIdentity, OidcError> {
        if self.has_introspection_credentials() {
            return self.introspect(token).await;
        }

        self.decode_jwt(token).await
    }

    async fn decode_jwt(&self, token: &str) -> Result<VerifiedIdentity, OidcError> {
        let header = decode_header(token).map_err(|error| {
            tracing::warn!(error = %error, "OIDC JWT header decode failed");
            OidcError::Unauthorized
        })?;

        let kid = header.kid.as_deref().ok_or_else(|| {
            tracing::warn!("OIDC JWT missing kid while issuer is configured");
            OidcError::Unauthorized
        })?;

        let jwks = self.jwks().await?;
        if let Some(identity) = self.decode_with_jwks(token, &header, kid, &jwks) {
            return Ok(identity);
        }

        let jwks = self.refresh_jwks().await?;
        self.decode_with_jwks(token, &header, kid, &jwks)
            .ok_or_else(|| {
                tracing::warn!(kid, "OIDC JWT kid not found in JWKS after refresh");
                OidcError::Unauthorized
            })
    }

    fn decode_with_jwks(
        &self,
        token: &str,
        header: &Header,
        kid: &str,
        jwks: &JwkSet,
    ) -> Option<VerifiedIdentity> {
        if header.alg != Algorithm::RS256 {
            tracing::warn!(algorithm = ?header.alg, "OIDC JWT algorithm is not allowed");
            return None;
        }

        let jwk = jwks.find(kid)?;
        let decoding_key = DecodingKey::from_jwk(jwk)
            .map_err(|error| {
                tracing::warn!(error = %error, kid, "OIDC JWT failed to build decoding key from JWKS");
                OidcError::Unauthorized
            })
            .ok()?;

        let mut validation = Validation::new(Algorithm::RS256);
        validation.algorithms = vec![Algorithm::RS256];
        validation.set_issuer(&[self.config.issuer.as_str()]);
        if self.config.audience.trim().is_empty() {
            validation.validate_aud = false;
        } else {
            validation.set_audience(&[self.config.audience.as_str()]);
        }

        decode::<IdTokenClaims>(token, &decoding_key, &validation)
            .map(|token_data| identity_from_claims(token_data.claims))
            .map_err(|error| {
                tracing::warn!(error = %error, issuer = %self.config.issuer, "OIDC JWT validation failed");
                OidcError::Unauthorized
            })
            .ok()
    }

    async fn introspect(&self, token: &str) -> Result<VerifiedIdentity, OidcError> {
        let introspection_url = self.introspection_url().await?;
        let response = self
            .client
            .post(&introspection_url)
            .basic_auth(
                self.config.introspection_client_id.as_str(),
                Some(self.config.introspection_client_secret.as_str()),
            )
            .form(&[("token", token), ("token_type_hint", "access_token")])
            .send()
            .await
            .map_err(|error| {
                tracing::warn!(error = %error, url = %introspection_url, "OIDC introspection failed");
                OidcError::Unauthorized
            })?;

        if !response.status().is_success() {
            tracing::warn!(status = %response.status(), url = %introspection_url, "OIDC introspection returned non-success");
            return Err(OidcError::Unauthorized);
        }

        let body: IntrospectionResponse = response.json().await.map_err(|error| {
            tracing::warn!(error = %error, url = %introspection_url, "OIDC introspection decode failed");
            OidcError::Unauthorized
        })?;

        if !body.active {
            tracing::warn!("OIDC introspection rejected inactive token");
            return Err(OidcError::Unauthorized);
        }

        let sub = body.sub.or(body.username).ok_or_else(|| {
            tracing::warn!("OIDC introspection response missing subject");
            OidcError::Unauthorized
        })?;

        self.ensure_audience_matches(&body.aud)?;

        Ok(VerifiedIdentity {
            sub,
            tenant_id: None,
            roles: Vec::new(),
            email: None,
            issuer: body.iss,
            audience: body.aud,
        })
    }

    async fn introspection_url(&self) -> Result<String, OidcError> {
        if !self.config.introspection_url.trim().is_empty() {
            return Ok(self.config.introspection_url.clone());
        }

        let discovery = self.discovery().await?;
        discovery.introspection_endpoint.ok_or_else(|| {
            tracing::warn!("OIDC discovery missing introspection endpoint");
            OidcError::Unauthorized
        })
    }

    async fn jwks(&self) -> Result<JwkSet, OidcError> {
        if let Some(jwks) = self.jwks.read().await.clone() {
            return Ok(jwks);
        }

        self.refresh_jwks().await
    }

    async fn refresh_jwks(&self) -> Result<JwkSet, OidcError> {
        let discovery = self.discovery().await?;
        let jwks: JwkSet = self.fetch_json(&discovery.jwks_uri).await?;
        *self.jwks.write().await = Some(jwks.clone());
        Ok(jwks)
    }

    async fn discovery(&self) -> Result<OpenIdConfiguration, OidcError> {
        if let Some(discovery) = self.discovery.read().await.clone() {
            return Ok(discovery);
        }

        let discovery_url = format!(
            "{}/.well-known/openid-configuration",
            self.config.issuer.trim_end_matches('/')
        );
        let discovery: OpenIdConfiguration = self.fetch_json(&discovery_url).await?;
        *self.discovery.write().await = Some(discovery.clone());
        Ok(discovery)
    }

    async fn fetch_json<T>(&self, url: &str) -> Result<T, OidcError>
    where
        T: DeserializeOwned,
    {
        let response = tokio::time::timeout(Duration::from_secs(5), self.client.get(url).send())
            .await
            .map_err(|_| {
                tracing::warn!(url, "OIDC fetch timed out");
                OidcError::Unauthorized
            })?
            .map_err(|error| {
                tracing::warn!(error = %error, url, "OIDC fetch failed");
                OidcError::Unauthorized
            })?;

        if !response.status().is_success() {
            tracing::warn!(status = %response.status(), url, "OIDC endpoint returned non-success");
            return Err(OidcError::Unauthorized);
        }

        response.json::<T>().await.map_err(|error| {
            tracing::warn!(error = %error, url, "OIDC JSON decode failed");
            OidcError::Unauthorized
        })
    }

    fn has_introspection_credentials(&self) -> bool {
        !self.config.introspection_client_id.trim().is_empty()
            && !self.config.introspection_client_secret.trim().is_empty()
    }

    fn ensure_audience_matches(
        &self,
        audience: &Option<serde_json::Value>,
    ) -> Result<(), OidcError> {
        if self.config.audience.trim().is_empty() {
            return Ok(());
        }

        let expected = &self.config.audience;
        let matches = match audience {
            Some(serde_json::Value::String(value)) => value == expected,
            Some(serde_json::Value::Array(values)) => values
                .iter()
                .any(|value| value.as_str().map(|item| item == expected).unwrap_or(false)),
            _ => false,
        };

        matches.then_some(()).ok_or_else(|| {
            tracing::warn!(expected = %expected, "OIDC introspection audience mismatch");
            OidcError::Unauthorized
        })
    }
}

#[derive(Debug)]
pub enum OidcError {
    Unauthorized,
}

fn identity_from_claims(claims: IdTokenClaims) -> VerifiedIdentity {
    VerifiedIdentity {
        sub: claims.sub,
        tenant_id: claims.tenant_id,
        roles: claims.roles.unwrap_or_default(),
        email: claims.email,
        issuer: claims.iss,
        audience: claims.aud,
    }
}
