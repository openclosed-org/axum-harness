use crate::commercial::LibSqlEntitlementProjection;
use crate::config::Config;
use crate::error::BffError;
use crate::state::DatabaseBackend;
use data::ports::lib_sql::LibSqlPort;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use storage_turso::TursoBackend;
use subtle::ConstantTimeEq;

pub const BILLING_LEDGER_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS commercial_billing_webhook_events (
    provider TEXT NOT NULL,
    provider_event_id TEXT NOT NULL,
    event_type TEXT NOT NULL,
    payload_hash TEXT NOT NULL,
    status TEXT NOT NULL,
    attempt_count INTEGER NOT NULL,
    last_error TEXT,
    received_at TEXT NOT NULL,
    processed_at TEXT,
    PRIMARY KEY (provider, provider_event_id)
);

CREATE INDEX IF NOT EXISTS idx_commercial_billing_webhook_events_received
    ON commercial_billing_webhook_events(provider, received_at);
"#;

#[derive(Clone)]
pub struct BillingStack {
    provider: BillingProvider,
}

#[derive(Clone)]
enum BillingProvider {
    Disabled,
    Creem(Arc<CreemBillingProvider>),
}

#[derive(Debug, Serialize)]
pub struct CheckoutResponse {
    pub checkout_url: String,
}

impl BillingStack {
    pub fn disabled() -> Self {
        Self {
            provider: BillingProvider::Disabled,
        }
    }

    pub fn creem_for_test(config: Config, db: DatabaseBackend) -> anyhow::Result<Self> {
        Ok(Self {
            provider: BillingProvider::Creem(Arc::new(CreemBillingProvider::new(
                CreemConfig::from_app_config(&config),
                match db {
                    DatabaseBackend::Embedded(db) => TursoBackend::Embedded(db),
                    DatabaseBackend::Remote(db) => TursoBackend::Remote(db),
                },
            ))),
        })
    }

    pub fn from_config(config: &Config, db: Option<DatabaseBackend>) -> anyhow::Result<Self> {
        if config.billing_provider.eq_ignore_ascii_case("disabled") {
            return Ok(Self::disabled());
        }

        if !config.billing_provider.eq_ignore_ascii_case("creem") {
            anyhow::bail!(
                "unsupported APP_BILLING_PROVIDER: {}",
                config.billing_provider
            );
        }

        let backend = match db {
            Some(DatabaseBackend::Embedded(db)) => TursoBackend::Embedded(db),
            Some(DatabaseBackend::Remote(db)) => TursoBackend::Remote(db),
            None => anyhow::bail!("APP_BILLING_PROVIDER=creem requires database configuration"),
        };

        Ok(Self {
            provider: BillingProvider::Creem(Arc::new(CreemBillingProvider::new(
                CreemConfig::from_app_config(config),
                backend,
            ))),
        })
    }

    pub fn creem(&self) -> Result<Arc<CreemBillingProvider>, BffError> {
        match &self.provider {
            BillingProvider::Creem(provider) => Ok(provider.clone()),
            BillingProvider::Disabled => Err(BffError::NotFound(
                "Creem billing provider is not enabled".to_string(),
            )),
        }
    }
}

#[derive(Clone)]
struct CreemConfig {
    api_key: String,
    webhook_secret: String,
    base_url: String,
    product_counter_pro: String,
    public_base_url: String,
    counter_paid_capability: String,
}

impl CreemConfig {
    fn from_app_config(config: &Config) -> Self {
        let base_url = if config.creem_env.eq_ignore_ascii_case("live") {
            "https://api.creem.io/v1"
        } else {
            "https://test-api.creem.io/v1"
        };

        Self {
            api_key: config.creem_api_key.clone(),
            webhook_secret: config.creem_webhook_secret.clone(),
            base_url: base_url.to_string(),
            product_counter_pro: config.creem_product_counter_pro.clone(),
            public_base_url: config.public_base_url.trim_end_matches('/').to_string(),
            counter_paid_capability: config.counter_paid_capability.clone(),
        }
    }
}

pub struct CreemBillingProvider {
    config: CreemConfig,
    port: TursoBackend,
    entitlement: LibSqlEntitlementProjection,
}

impl CreemBillingProvider {
    fn new(config: CreemConfig, port: TursoBackend) -> Self {
        Self {
            config,
            entitlement: LibSqlEntitlementProjection::new(port.clone()),
            port,
        }
    }

    pub async fn create_counter_checkout(
        &self,
        http_client: reqwest::Client,
        tenant_id: &str,
        subject_id: &str,
    ) -> Result<CheckoutResponse, BffError> {
        #[derive(Serialize)]
        struct CreemCheckoutRequest<'a> {
            product_id: &'a str,
            success_url: String,
            metadata: serde_json::Value,
        }

        #[derive(Deserialize)]
        struct CreemCheckoutResponse {
            checkout_url: String,
        }

        let response = http_client
            .post(format!("{}/checkouts", self.config.base_url))
            .header("x-api-key", &self.config.api_key)
            .json(&CreemCheckoutRequest {
                product_id: &self.config.product_counter_pro,
                success_url: format!("{}/billing/success", self.config.public_base_url),
                metadata: serde_json::json!({
                    "tenant_id": tenant_id,
                    "subject_id": subject_id,
                    "capability": self.config.counter_paid_capability,
                }),
            })
            .send()
            .await
            .map_err(|error| {
                BffError::Dependency(format!("Creem checkout request failed: {error}"))
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(BffError::Dependency(format!(
                "Creem checkout request returned {status}: {body}"
            )));
        }

        let checkout = response
            .json::<CreemCheckoutResponse>()
            .await
            .map_err(|error| {
                BffError::Dependency(format!("Creem checkout response invalid: {error}"))
            })?;

        Ok(CheckoutResponse {
            checkout_url: checkout.checkout_url,
        })
    }

    pub async fn handle_webhook(
        &self,
        signature: &str,
        body: &[u8],
    ) -> Result<WebhookOutcome, BffError> {
        if !verify_creem_signature(&self.config.webhook_secret, body, signature) {
            return Err(BffError::Unauthorized(
                "Invalid Creem webhook signature".to_string(),
            ));
        }

        let event: CreemWebhookEvent = serde_json::from_slice(body).map_err(|error| {
            BffError::BadRequest(format!("Invalid Creem webhook JSON: {error}"))
        })?;
        let provider_event_id = event.provider_event_id();
        let payload_hash = sha256_hex(body);
        let received_at = chrono::Utc::now().to_rfc3339();
        let inserted = self
            .port
            .execute(
                "INSERT INTO commercial_billing_webhook_events (provider, provider_event_id, event_type, payload_hash, status, attempt_count, last_error, received_at, processed_at) VALUES ('creem', ?, ?, ?, 'received', 0, NULL, ?, NULL) ON CONFLICT(provider, provider_event_id) DO NOTHING",
                vec![
                    provider_event_id.clone(),
                    event.event_type.clone(),
                    payload_hash,
                    received_at,
                ],
            )
            .await
            .map_err(|error| BffError::Dependency(format!("Creem webhook ledger write failed: {error}")))?;

        if inserted == 0 {
            return Ok(WebhookOutcome::Duplicate);
        }

        let projection = self.apply_event(&event).await;
        match projection {
            Ok(()) => {
                self.mark_processed(&provider_event_id).await?;
                Ok(WebhookOutcome::Processed)
            }
            Err(error) => {
                self.mark_failed(&provider_event_id, &error.to_string())
                    .await?;
                Err(BffError::Dependency(format!(
                    "Creem webhook projection failed: {error}"
                )))
            }
        }
    }

    async fn apply_event(&self, event: &CreemWebhookEvent) -> anyhow::Result<()> {
        let Some(metadata) = event.commercial_metadata() else {
            return Ok(());
        };

        match event.event_type.as_str() {
            "checkout.completed" | "subscription.active" | "subscription.paid" => {
                self.entitlement
                    .grant(
                        &metadata.tenant_id,
                        &metadata.subject_id,
                        &metadata.capability,
                        "creem",
                        event.customer_id().as_deref(),
                        event.subscription_id().as_deref(),
                    )
                    .await
            }
            "subscription.canceled" | "subscription.expired" | "refund.created" => {
                self.entitlement
                    .revoke(
                        &metadata.tenant_id,
                        &metadata.subject_id,
                        &metadata.capability,
                        "creem",
                    )
                    .await
            }
            _ => Ok(()),
        }
    }

    async fn mark_processed(&self, provider_event_id: &str) -> Result<(), BffError> {
        self.port
            .execute(
                "UPDATE commercial_billing_webhook_events SET status = 'processed', attempt_count = attempt_count + 1, last_error = NULL, processed_at = ? WHERE provider = 'creem' AND provider_event_id = ?",
                vec![chrono::Utc::now().to_rfc3339(), provider_event_id.to_string()],
            )
            .await
            .map_err(|error| BffError::Dependency(format!("Creem webhook ledger update failed: {error}")))?;
        Ok(())
    }

    async fn mark_failed(&self, provider_event_id: &str, error: &str) -> Result<(), BffError> {
        self.port
            .execute(
                "UPDATE commercial_billing_webhook_events SET status = 'failed', attempt_count = attempt_count + 1, last_error = ? WHERE provider = 'creem' AND provider_event_id = ?",
                vec![error.to_string(), provider_event_id.to_string()],
            )
            .await
            .map_err(|error| BffError::Dependency(format!("Creem webhook ledger update failed: {error}")))?;
        Ok(())
    }

    pub async fn entitlement_active_for_test(
        &self,
        tenant_id: &str,
        subject_id: &str,
        capability: &str,
    ) -> anyhow::Result<bool> {
        self.entitlement
            .active_for_test(tenant_id, subject_id, capability)
            .await
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum WebhookOutcome {
    Processed,
    Duplicate,
}

#[derive(Debug, Deserialize)]
struct CreemWebhookEvent {
    #[serde(default)]
    id: Option<String>,
    #[serde(rename = "type", alias = "eventType", alias = "event_type")]
    event_type: String,
    #[serde(default)]
    data: serde_json::Value,
    #[serde(default, rename = "object")]
    object: serde_json::Value,
}

#[derive(Debug)]
struct CommercialMetadata {
    tenant_id: String,
    subject_id: String,
    capability: String,
}

impl CreemWebhookEvent {
    fn provider_event_id(&self) -> String {
        self.id
            .clone()
            .or_else(|| string_field(self.business_object(), "id"))
            .unwrap_or_else(|| sha256_hex(self.business_object().to_string().as_bytes()))
    }

    fn commercial_metadata(&self) -> Option<CommercialMetadata> {
        let metadata = self.metadata()?;
        Some(CommercialMetadata {
            tenant_id: string_field(metadata, "tenant_id")?,
            subject_id: string_field(metadata, "subject_id")
                .or_else(|| string_field(metadata, "user_sub"))?,
            capability: string_field(metadata, "capability")?,
        })
    }

    fn customer_id(&self) -> Option<String> {
        let object = self.business_object();
        string_field(object, "customer_id")
            .or_else(|| string_field(object, "customer"))
            .or_else(|| {
                object
                    .get("customer")
                    .and_then(|customer| string_field(customer, "id"))
            })
    }

    fn subscription_id(&self) -> Option<String> {
        let object = self.business_object();
        string_field(object, "subscription_id")
            .or_else(|| string_field(object, "subscription"))
            .or_else(|| {
                object
                    .get("subscription")
                    .and_then(|subscription| string_field(subscription, "id"))
            })
            .or_else(|| string_field(object, "id").filter(|id| id.starts_with("sub_")))
    }

    fn business_object(&self) -> &serde_json::Value {
        if !self.object.is_null() {
            return &self.object;
        }

        self.data.get("object").unwrap_or(&self.data)
    }

    fn metadata(&self) -> Option<&serde_json::Value> {
        let object = self.business_object();
        object
            .get("metadata")
            .or_else(|| {
                object
                    .get("checkout")
                    .and_then(|value| value.get("metadata"))
            })
            .or_else(|| {
                object
                    .get("subscription")
                    .and_then(|value| value.get("metadata"))
            })
            .or_else(|| self.data.get("metadata"))
            .or_else(|| self.object.get("metadata"))
    }
}

fn string_field(value: &serde_json::Value, key: &str) -> Option<String> {
    value.get(key)?.as_str().map(str::to_string)
}

pub fn verify_creem_signature(secret: &str, body: &[u8], signature: &str) -> bool {
    let expected = hmac_sha256_hex(secret.as_bytes(), body);
    expected
        .as_bytes()
        .ct_eq(signature.trim().as_bytes())
        .into()
}

pub fn creem_signature_for_test(secret: &str, body: &[u8]) -> String {
    hmac_sha256_hex(secret.as_bytes(), body)
}

fn hmac_sha256_hex(key: &[u8], message: &[u8]) -> String {
    const BLOCK_SIZE: usize = 64;
    let mut key_block = [0_u8; BLOCK_SIZE];
    if key.len() > BLOCK_SIZE {
        let hashed = Sha256::digest(key);
        key_block[..hashed.len()].copy_from_slice(&hashed);
    } else {
        key_block[..key.len()].copy_from_slice(key);
    }

    let mut outer_key_pad = [0x5c_u8; BLOCK_SIZE];
    let mut inner_key_pad = [0x36_u8; BLOCK_SIZE];
    for i in 0..BLOCK_SIZE {
        outer_key_pad[i] ^= key_block[i];
        inner_key_pad[i] ^= key_block[i];
    }

    let mut inner = Sha256::new();
    inner.update(inner_key_pad);
    inner.update(message);
    let inner_hash = inner.finalize();

    let mut outer = Sha256::new();
    outer.update(outer_key_pad);
    outer.update(inner_hash);
    hex::encode(outer.finalize())
}

fn sha256_hex(input: &[u8]) -> String {
    hex::encode(Sha256::digest(input))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creem_signature_verification_uses_hmac_sha256() {
        let body = br#"{"id":"evt_1","type":"checkout.completed"}"#;
        let signature = hmac_sha256_hex(b"secret", body);

        assert!(verify_creem_signature("secret", body, &signature));
        assert!(!verify_creem_signature("wrong", body, &signature));
    }

    #[test]
    fn creem_event_extracts_commercial_metadata() {
        let event: CreemWebhookEvent = serde_json::from_value(serde_json::json!({
            "id": "evt_1",
            "type": "checkout.completed",
            "data": {
                "metadata": {
                    "tenant_id": "tenant-a",
                    "subject_id": "user-a",
                    "capability": "counter.write"
                }
            }
        }))
        .unwrap();

        let metadata = event.commercial_metadata().unwrap();
        assert_eq!(metadata.tenant_id, "tenant-a");
        assert_eq!(metadata.subject_id, "user-a");
        assert_eq!(metadata.capability, "counter.write");
    }

    #[test]
    fn creem_event_accepts_dashboard_event_type_and_object_shape() {
        let event: CreemWebhookEvent = serde_json::from_value(serde_json::json!({
            "id": "evt_2iGTc600qGW6FBzloh2Nr7",
            "eventType": "subscription.canceled",
            "object": {
                "id": "sub_6pC2lNB6joCRQIZ1aMrTpi",
                "object": "subscription",
                "customer": {
                    "id": "cust_1OcIK1GEuVvXZwD19tjq2z"
                },
                "metadata": {
                    "tenant_id": "tenant-a",
                    "subject_id": "user-a",
                    "capability": "counter.write"
                }
            }
        }))
        .unwrap();

        assert_eq!(event.event_type, "subscription.canceled");
        assert_eq!(event.provider_event_id(), "evt_2iGTc600qGW6FBzloh2Nr7");
        assert_eq!(
            event.customer_id().as_deref(),
            Some("cust_1OcIK1GEuVvXZwD19tjq2z")
        );
        assert_eq!(
            event.subscription_id().as_deref(),
            Some("sub_6pC2lNB6joCRQIZ1aMrTpi")
        );
        assert!(event.commercial_metadata().is_some());
    }

    #[test]
    fn creem_refund_event_finds_nested_checkout_metadata() {
        let event: CreemWebhookEvent = serde_json::from_value(serde_json::json!({
            "id": "evt_refund",
            "eventType": "refund.created",
            "object": {
                "id": "ref_1",
                "subscription": {
                    "id": "sub_1"
                },
                "checkout": {
                    "id": "ch_1",
                    "metadata": {
                        "tenant_id": "tenant-a",
                        "subject_id": "user-a",
                        "capability": "counter.write"
                    }
                }
            }
        }))
        .unwrap();

        let metadata = event.commercial_metadata().unwrap();
        assert_eq!(metadata.tenant_id, "tenant-a");
        assert_eq!(metadata.subject_id, "user-a");
        assert_eq!(metadata.capability, "counter.write");
        assert_eq!(event.subscription_id().as_deref(), Some("sub_1"));
    }
}
