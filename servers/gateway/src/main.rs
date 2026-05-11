//! Pingora-based reverse proxy for backend API traffic.
//!
//! Routing rules:
//!   - `/healthz`      → health check (200 OK, handled locally)
//!   - `/*`            → web-bff upstream (3010)
//!
//! Environment variables:
//!   API_UPSTREAM  — web-bff server address (default: 127.0.0.1:3010)
//!   BIND          — Bind address (default: 0.0.0.0:3000)

use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use pingora::prelude::*;
use pingora_core::upstreams::peer::HttpPeer;
use pingora_http::ResponseHeader;
use pingora_load_balancing::{
    Backend, LoadBalancer, health_check::TcpHealthCheck, selection::RoundRobin,
};
use pingora_proxy::{ProxyHttp, Session, http_proxy_service};
use serde::{Deserialize, Serialize};
use std::time::Instant;

// ── Configuration ────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GatewayConfig {
    api_upstream: String,
    bind: String,
}

impl GatewayConfig {
    fn from_env() -> anyhow::Result<Self> {
        platform::load_config(Self::default(), "GATEWAY_", Some("GATEWAY_CONFIG_FILE"))
            .map_err(Into::into)
    }
}

impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
            api_upstream: "127.0.0.1:3010".to_string(),
            bind: "0.0.0.0:3000".to_string(),
        }
    }
}

// ── Gateway proxy ────────────────────────────────────────────

struct Gateway {
    api_upstreams: Arc<LoadBalancer<RoundRobin>>,
    readiness_client: reqwest::Client,
    config: GatewayConfig,
}

impl Gateway {
    fn new(config: &GatewayConfig) -> Self {
        // API upstream (web-bff) with health check
        let mut api_upstreams =
            LoadBalancer::<RoundRobin>::try_from_iter([config.api_upstream.as_str()])
                .expect("valid API upstream address");
        let api_hc = TcpHealthCheck::new();
        api_upstreams.set_health_check(api_hc);
        api_upstreams.health_check_frequency = Some(std::time::Duration::from_secs(5));
        let api_bg = background_service("api-health-check", api_upstreams);
        let api_upstreams = api_bg.task();

        let readiness_client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(2))
            .build()
            .unwrap_or_default();

        Self {
            api_upstreams,
            readiness_client,
            config: config.clone(),
        }
    }
}

/// Per-request context — tracks start time for latency logging.
struct RequestCtx {
    start: Instant,
}

#[async_trait]
impl ProxyHttp for Gateway {
    type CTX = RequestCtx;

    fn new_ctx(&self) -> Self::CTX {
        RequestCtx {
            start: Instant::now(),
        }
    }

    /// Handle health check directly (short-circuit, no upstream).
    async fn request_filter(&self, session: &mut Session, _ctx: &mut Self::CTX) -> Result<bool> {
        let path = session.req_header().uri.path();

        if path == "/healthz" || path == "/health" {
            let body_str = serde_json::json!({
                "status": "ok",
                "upstreams": {
                    "api": "configured"
                }
            })
            .to_string();

            let body = Bytes::from(body_str);
            let len = body.len();
            let mut hdr = ResponseHeader::build(200, Some(len))?;
            hdr.set_content_length(len)?;
            hdr.insert_header("content-type", "application/json")?;

            session.write_response_header(Box::new(hdr), false).await?;
            session.write_response_body(Some(body), true).await?;
            return Ok(true);
        }

        if path == "/readyz" {
            // Check upstream readiness by probing each upstream via TCP.
            let api_ready = self
                .readiness_client
                .get(format!("http://{}/healthz", self.config.api_upstream))
                .send()
                .await
                .is_ok();
            let status_code = if api_ready { 200 } else { 503 };

            let body_str = serde_json::json!({
                "status": if api_ready { "ready" } else { "not_ready" },
                "upstreams": {
                    "api": if api_ready { "healthy" } else { "unhealthy" },
                }
            })
            .to_string();

            let body = Bytes::from(body_str);
            let len = body.len();
            let mut hdr = ResponseHeader::build(status_code, Some(len))?;
            hdr.set_content_length(len)?;
            hdr.insert_header("content-type", "application/json")?;

            session.write_response_header(Box::new(hdr), false).await?;
            session.write_response_body(Some(body), true).await?;
            return Ok(true);
        }

        Ok(false)
    }

    /// Select upstream based on request path.
    async fn upstream_peer(
        &self,
        session: &mut Session,
        _ctx: &mut Self::CTX,
    ) -> Result<Box<HttpPeer>> {
        let backend = self.api_upstreams.select(b"", 256).unwrap();
        let sni = "";
        Ok(Box::new(HttpPeer::new(backend, false, sni.to_string())))
    }

    /// Log proxy transaction — method, path, upstream, status, latency.
    async fn logging(
        &self,
        session: &mut Session,
        error: Option<&pingora_core::Error>,
        ctx: &mut Self::CTX,
    ) {
        let latency = ctx.start.elapsed();
        let method = session.req_header().method.as_str();
        let path = session.req_header().uri.path();

        if let Some(resp) = session.response_written() {
            let status = resp.status.as_u16();
            if status >= 400 || error.is_some() {
                tracing::warn!(
                    method,
                    path,
                    status,
                    latency_ms = latency.as_millis(),
                    error = ?error.map(|e| e.to_string()),
                    "proxy_request"
                );
            } else {
                tracing::info!(
                    method,
                    path,
                    status,
                    latency_ms = latency.as_millis(),
                    "proxy_request"
                );
            }
        } else if let Some(e) = error {
            tracing::error!(
                method,
                path,
                latency_ms = latency.as_millis(),
                error = %e,
                "proxy_request_failed"
            );
        }
    }
}

// ── Entry point ──────────────────────────────────────────────

fn main() -> anyhow::Result<()> {
    let _observability = observability::init_observability("gateway", "info,gateway=debug")
        .map_err(std::io::Error::other)?;

    let config = GatewayConfig::from_env()?;
    let bind_addr = config.bind.clone();

    tracing::info!("Starting Pingora gateway on {}", bind_addr);
    tracing::info!("  API upstream (web-bff):    {}", config.api_upstream);

    let mut server = Server::new(Some(Opt::parse_args()))?;
    server.bootstrap();

    let gateway = Gateway::new(&config);

    let mut proxy = http_proxy_service(&server.configuration, gateway);
    proxy.add_tcp(&bind_addr);

    server.add_service(proxy);
    server.run_forever()
}
