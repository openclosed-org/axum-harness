//! Outbound delivery worker.
//!
//! Polls notification and outbound webhook delivery jobs, applies retry policy,
//! and sends exhausted failures to the dead-letter path. Provider transports are
//! intentionally not wired in this reference entrypoint yet.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::{Router, routing::get};
use notification::{DeliveryWorker, InMemoryDeliveryOutbox, NoopOutboundTransport, RetryPolicy};
use tokio::sync::RwLock;
use tracing::{info, warn};

struct WorkerState {
    healthy: RwLock<bool>,
    delivered_count: RwLock<u64>,
}

impl WorkerState {
    fn new() -> Self {
        Self {
            healthy: RwLock::new(true),
            delivered_count: RwLock::new(0),
        }
    }
}

async fn healthz(state: axum::extract::State<Arc<WorkerState>>) -> axum::Json<serde_json::Value> {
    let delivered = state.delivered_count.read().await;
    axum::Json(serde_json::json!({
        "status": "ok",
        "delivered_count": *delivered,
    }))
}

async fn readyz(state: axum::extract::State<Arc<WorkerState>>) -> axum::Json<serde_json::Value> {
    if *state.healthy.read().await {
        axum::Json(serde_json::json!({ "status": "ready" }))
    } else {
        axum::Json(serde_json::json!({ "status": "not ready" }))
    }
}

async fn start_health_server(state: Arc<WorkerState>, addr: SocketAddr) -> anyhow::Result<()> {
    let app = Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!(%addr, "outbound delivery health server listening");
    axum::serve(listener, app).await?;
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _observability = observability::init_observability(
        "outbound-delivery-worker",
        "outbound_delivery_worker=info",
    )
    .map_err(anyhow::Error::msg)?;

    let state = Arc::new(WorkerState::new());
    let health_state = state.clone();
    tokio::spawn(async move {
        let addr: SocketAddr = "0.0.0.0:3036".parse().expect("static health address");
        if let Err(error) = start_health_server(health_state, addr).await {
            warn!(%error, "outbound delivery health server failed");
        }
    });

    let outbox = Arc::new(InMemoryDeliveryOutbox::default());
    let transport = Arc::new(NoopOutboundTransport);
    let worker = DeliveryWorker::new(outbox, transport, RetryPolicy::default());

    info!("outbound delivery worker running in noop transport mode");
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
    loop {
        interval.tick().await;
        let delivered = worker.deliver_due(50).await?;
        if delivered > 0 {
            *state.delivered_count.write().await += delivered as u64;
        }
    }
}
