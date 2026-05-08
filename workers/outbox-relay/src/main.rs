//! Outbox relay worker — main entry point.
//!
//! This worker polls the outbox table, publishes events to the event bus,
//! and tracks checkpoints and deduplication.
//!
//! Configuration is loaded via SOPS-encrypted secrets, never from `.env` files.
//! For local development: `just sops-run outbox-relay-worker`

#![deny(unused_imports, unused_variables)]

use event_bus::adapters::nats_bus::NatsEventBus;
use runtime::adapters::nats::NatsPubSub;
use storage_turso::TursoBackend;
use tracing::{error, info};
use worker_runtime::{
    bootstrap_worker, build_worker_store_set, shutdown_signal, spawn_health_server,
};

mod checkpoint;
mod config;
mod dedupe;
mod idempotency;
mod polling;
mod publish;

use config::Config;
use polling::{OutboxPoller, OutboxReader, PollerConfig};
use publish::OutboxPublisher;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load configuration from SOPS-encrypted environment variables
    let config = Config::from_env()?;

    let worker_runtime::WorkerBootstrap {
        observability: _observability,
        state,
    } = bootstrap_worker("outbox-relay-worker", &config.rust_log)?;

    info!("Outbox relay worker starting");
    info!("Database: {}", config.database_url);
    info!("NATS URL: {}", config.nats_url);
    info!("Poll interval: {:?}", config.poll_interval());

    // Start health check server using config
    spawn_health_server(state.clone(), config.health_addr(), "outbox-relay-worker");

    let event_bus = NatsEventBus::connect(&config.nats_url, &config.nats_subject_prefix).await?;
    let pubsub = NatsPubSub::connect(&config.nats_url, &config.nats_subject_prefix).await?;

    // Create outbox publisher with both event bus and pubsub
    let publisher = OutboxPublisher::new(event_bus, pubsub);

    let stores = build_worker_store_set(
        "outbox-relay-worker",
        &config.database_url,
        config.turso_auth_token.as_deref(),
        &config.checkpoint_path,
    )
    .await?;
    let worker_runtime::WorkerStoreSet {
        checkpoint,
        idempotency: idempotency_store,
        dedupe,
        backend: store_backend,
    } = stores;

    // Create the production reader from the configured database.
    let mut poller = create_outbox_poller(&config, checkpoint, dedupe).await?;

    info!(
        "Outbox relay worker running (poll interval: {:?}, batch size: {}, checkpoint: {}, store_backend: {})",
        config.poll_interval(),
        config.batch_size,
        config.checkpoint_path,
        store_backend.as_str(),
    );

    // Main processing loop
    loop {
        tokio::select! {
            _ = shutdown_signal() => {
                state.set_healthy(false).await;
                info!("shutdown signal received, stopping outbox relay worker");
                break;
            }
            _ = tokio::time::sleep(config.poll_interval()) => {
                let entries = poller.poll_cycle().await;

                if !entries.is_empty() {
                    // Filter out already-processed entries via idempotency store
                    let mut to_publish = Vec::new();
                    for entry in &entries {
                        if idempotency_store
                            .is_already_processed(&entry.id)
                            .await
                            .unwrap_or(false)
                        {
                            info!(entry_id = %entry.id, "skipping already-processed entry");
                            continue;
                        }
                        if idempotency_store.start(&entry.id).await.unwrap_or(false) {
                            to_publish.push(entry.clone());
                        } else {
                            info!(entry_id = %entry.id, "skipping entry already in progress");
                        }
                    }

                    if !to_publish.is_empty() {
                        let (successes, failures) = publisher.publish_batch(&to_publish).await;
                        state.record_count("processed_count", successes.len()).await;
                        state.record_count("failed_count", failures.len()).await;

                        // Mark published in database for successful entries
                        let mut completed = Vec::new();
                        if !successes.is_empty() {
                            if let Err(e) = poller.mark_published(&successes).await {
                                error!(error = %e, "failed to mark outbox entries as published");
                                for id in &successes {
                                    let _ = idempotency_store
                                        .fail(id, format!("mark_published failed: {}", e))
                                        .await;
                                }
                            } else {
                                for id in &successes {
                                    let _ = idempotency_store.complete(id).await;
                                }
                                completed.extend(successes.clone());
                            }
                        }

                        // Mark failures in idempotency store
                        for (id, err) in &failures {
                            let _ = idempotency_store.fail(id, err.to_string()).await;
                        }

                        // Advance only after publish and database state are durable.
                        poller.mark_processed_by_id(&entries, &completed).await;
                    }
                }
            }
        }
    }

    Ok(())
}

/// Create the production outbox poller from the configured database.
async fn create_outbox_poller(
    config: &Config,
    checkpoint: Box<dyn worker_runtime::CheckpointStore>,
    dedupe: Box<dyn worker_runtime::DedupeStore>,
) -> anyhow::Result<OutboxPoller<Box<dyn OutboxReader>>> {
    let poller_config = PollerConfig {
        poll_interval: config.poll_interval(),
        batch_size: config.batch_size,
    };

    let turso = TursoBackend::connect(&config.database_url, config.turso_auth_token.as_deref())
        .await
        .map_err(|e| {
            anyhow::anyhow!(
                "failed to open outbox database '{}': {e}",
                config.database_url
            )
        })?;
    let reader = polling::LibSqlOutboxReader::new(turso);

    Ok(OutboxPoller::new(
        Box::new(reader),
        poller_config,
        checkpoint,
        dedupe,
    ))
}
