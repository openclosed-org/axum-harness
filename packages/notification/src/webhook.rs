use async_trait::async_trait;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use uuid::Uuid;

use crate::outbox::{DeliveryJob, DeliveryOutbox, DeliveryOutboxError};
use crate::retry::RetryPolicy;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebhookEndpoint {
    pub id: String,
    pub tenant_id: String,
    pub url: String,
    pub signing_secret: String,
    pub enabled: bool,
}

impl WebhookEndpoint {
    pub fn new(
        tenant_id: impl Into<String>,
        id: impl Into<String>,
        url: impl Into<String>,
        signing_secret: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            tenant_id: tenant_id.into(),
            url: url.into(),
            signing_secret: signing_secret.into(),
            enabled: true,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum WebhookRegistryError {
    #[error("webhook endpoint not found: {0}")]
    NotFound(String),
    #[error("webhook registry error: {0}")]
    Store(String),
}

#[async_trait]
pub trait WebhookRegistry: Send + Sync {
    async fn register(&self, endpoint: WebhookEndpoint) -> Result<(), WebhookRegistryError>;
    async fn endpoints_for_tenant(
        &self,
        tenant_id: &str,
    ) -> Result<Vec<WebhookEndpoint>, WebhookRegistryError>;
}

#[derive(Debug, Default)]
pub struct InMemoryWebhookRegistry {
    endpoints: tokio::sync::Mutex<HashMap<String, WebhookEndpoint>>,
}

#[async_trait]
impl WebhookRegistry for InMemoryWebhookRegistry {
    async fn register(&self, endpoint: WebhookEndpoint) -> Result<(), WebhookRegistryError> {
        self.endpoints
            .lock()
            .await
            .insert(endpoint.id.clone(), endpoint);
        Ok(())
    }

    async fn endpoints_for_tenant(
        &self,
        tenant_id: &str,
    ) -> Result<Vec<WebhookEndpoint>, WebhookRegistryError> {
        Ok(self
            .endpoints
            .lock()
            .await
            .values()
            .filter(|endpoint| endpoint.tenant_id == tenant_id && endpoint.enabled)
            .cloned()
            .collect())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutboundWebhookEvent {
    pub id: Uuid,
    pub tenant_id: String,
    pub event_type: String,
    pub payload: serde_json::Value,
}

impl OutboundWebhookEvent {
    pub fn new(
        tenant_id: impl Into<String>,
        event_type: impl Into<String>,
        payload: serde_json::Value,
    ) -> Self {
        Self {
            id: Uuid::now_v7(),
            tenant_id: tenant_id.into(),
            event_type: event_type.into(),
            payload,
        }
    }
}

#[async_trait]
pub trait OutboundWebhookPort: Send + Sync {
    async fn publish(
        &self,
        event: OutboundWebhookEvent,
    ) -> Result<Vec<DeliveryJob>, DeliveryOutboxError>;
}

pub struct OutboxWebhookPublisher {
    registry: Arc<dyn WebhookRegistry>,
    outbox: Arc<dyn DeliveryOutbox>,
    retry_policy: RetryPolicy,
}

impl OutboxWebhookPublisher {
    pub fn new(
        registry: Arc<dyn WebhookRegistry>,
        outbox: Arc<dyn DeliveryOutbox>,
        retry_policy: RetryPolicy,
    ) -> Self {
        Self {
            registry,
            outbox,
            retry_policy,
        }
    }
}

#[async_trait]
impl OutboundWebhookPort for OutboxWebhookPublisher {
    async fn publish(
        &self,
        event: OutboundWebhookEvent,
    ) -> Result<Vec<DeliveryJob>, DeliveryOutboxError> {
        let endpoints = self
            .registry
            .endpoints_for_tenant(&event.tenant_id)
            .await
            .map_err(|error| DeliveryOutboxError::Store(error.to_string()))?;
        let mut jobs = Vec::with_capacity(endpoints.len());

        for endpoint in endpoints {
            let mut headers = BTreeMap::new();
            headers.insert("x-axum-harness-event".to_string(), event.event_type.clone());
            headers.insert("x-axum-harness-event-id".to_string(), event.id.to_string());
            let job = DeliveryJob::webhook(
                event.tenant_id.clone(),
                endpoint.id,
                endpoint.url,
                Some(endpoint.signing_secret),
                event.payload.clone(),
                headers,
                self.retry_policy,
            );
            jobs.push(self.outbox.enqueue(job).await?);
        }

        Ok(jobs)
    }
}

pub struct HmacSha256WebhookSigner;

impl HmacSha256WebhookSigner {
    pub const HEADER: &'static str = "x-axum-harness-signature";

    pub fn sign(secret: &str, body: &[u8]) -> String {
        let mut mac =
            HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC accepts keys of any length");
        mac.update(body);
        format!("sha256={}", hex::encode(mac.finalize().into_bytes()))
    }

    pub fn verify(secret: &str, body: &[u8], signature: &str) -> bool {
        let Some(hex_signature) = signature.strip_prefix("sha256=") else {
            return false;
        };
        let Ok(signature_bytes) = hex::decode(hex_signature) else {
            return false;
        };
        let Ok(mut mac) = HmacSha256::new_from_slice(secret.as_bytes()) else {
            return false;
        };
        mac.update(body);
        mac.verify_slice(&signature_bytes).is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::outbox::InMemoryDeliveryOutbox;

    #[test]
    fn webhook_signatures_verify_body_and_secret() {
        let body = br#"{"event":"counter.updated"}"#;
        let signature = HmacSha256WebhookSigner::sign("secret", body);

        assert!(HmacSha256WebhookSigner::verify("secret", body, &signature));
        assert!(!HmacSha256WebhookSigner::verify("wrong", body, &signature));
        assert!(!HmacSha256WebhookSigner::verify(
            "secret",
            br#"{"event":"changed"}"#,
            &signature
        ));
    }

    #[tokio::test]
    async fn outbound_webhook_publish_enqueues_one_job_per_enabled_endpoint() {
        let registry = Arc::new(InMemoryWebhookRegistry::default());
        registry
            .register(WebhookEndpoint::new(
                "tenant-a",
                "endpoint-a",
                "https://example.test/webhook",
                "secret",
            ))
            .await
            .unwrap();
        let outbox = Arc::new(InMemoryDeliveryOutbox::default());
        let publisher =
            OutboxWebhookPublisher::new(registry, outbox.clone(), RetryPolicy::no_retry());

        let jobs = publisher
            .publish(OutboundWebhookEvent::new(
                "tenant-a",
                "counter.updated",
                serde_json::json!({ "value": 1 }),
            ))
            .await
            .unwrap();

        assert_eq!(jobs.len(), 1);
        assert_eq!(
            outbox
                .claim_due(10, chrono::Utc::now())
                .await
                .unwrap()
                .len(),
            1
        );
    }
}
