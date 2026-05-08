//! Projector worker — builds read models from event streams.
//!
//! Consumes replayable events from the outbox, runs them through interested
//! consumers, and updates materialized read models.

#![deny(unused_imports, unused_variables)]

use event_bus::ports::EventEnvelope;
use storage_turso::TursoBackend;
use tracing::info;
use worker_runtime::{
    FileCheckpointStore, WorkerHealthState, bootstrap_worker, build_checkpoint_store,
    shutdown_signal, spawn_health_server,
};

mod checkpoint;
mod config;
mod consumers;
mod error;
mod live;
mod readmodels;
mod replay;
mod source;

use checkpoint::ProjectionCheckpointPort;
use config::Config;
use consumers::EventConsumer;
use error::ProjectorError;
use live::LiveEventSubscriber;
use readmodels::{ReadModel, SqliteCounterReadModel};
use replay::{ReplayManager, ReplayStrategy};
use source::CounterOutboxSource;

/// The projector — consumes events and updates read models.
pub struct Projector {
    consumers: Vec<Box<dyn EventConsumer>>,
    read_models: Vec<Box<dyn ReadModel>>,
    checkpoint: Box<dyn ProjectionCheckpointPort>,
}

impl Projector {
    pub fn new() -> Self {
        Self {
            consumers: Vec::new(),
            read_models: Vec::new(),
            checkpoint: Box::new(FileCheckpointStore::new(
                "/tmp/projector-checkpoint.json",
                0,
            )),
        }
    }

    pub fn with_checkpoint(checkpoint: Box<dyn ProjectionCheckpointPort>) -> Self {
        Self {
            consumers: Vec::new(),
            read_models: Vec::new(),
            checkpoint,
        }
    }

    pub fn add_consumer(&mut self, consumer: Box<dyn EventConsumer>) {
        self.consumers.push(consumer);
    }

    pub fn add_read_model(&mut self, model: Box<dyn ReadModel>) {
        self.read_models.push(model);
    }

    /// Process a single event through the projection pipeline.
    pub async fn process_event(&self, envelope: &EventEnvelope) -> Result<usize, ProjectorError> {
        let mut projected = 0;

        for consumer in &self.consumers {
            if consumer.is_interested(&envelope.event)
                && let Some(update) = consumer.consume(envelope).await?
            {
                for model in &self.read_models {
                    model.apply_update(&update).await?;
                    projected += 1;
                }
            }
        }

        Ok(projected)
    }

    pub fn checkpoint(&self) -> &dyn ProjectionCheckpointPort {
        self.checkpoint.as_ref()
    }
}

impl Default for Projector {
    fn default() -> Self {
        Self::new()
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = Config::from_env()?;

    let worker_runtime::WorkerBootstrap {
        observability: _observability,
        state,
    } = bootstrap_worker("projector-worker", &config.rust_log)?;

    info!("Projector worker starting");
    info!("Database: {}", config.database_url);

    spawn_health_server(state.clone(), config.health_addr(), "projector-worker");

    let db = TursoBackend::connect(&config.database_url, config.turso_auth_token.as_deref())
        .await
        .map_err(|e| {
            anyhow::anyhow!(
                "failed to open projector database '{}': {e}",
                config.database_url
            )
        })?;
    let source = CounterOutboxSource::new(db.clone());
    let sqlite_read_model = SqliteCounterReadModel::new(db.clone());
    sqlite_read_model.init().await?;

    let (checkpoint, checkpoint_backend) = build_checkpoint_store(
        "projector-worker",
        &config.database_url,
        config.turso_auth_token.as_deref(),
        &config.checkpoint_path,
        0,
    )
    .await?;

    let mut projector = Projector::with_checkpoint(checkpoint);
    projector.add_consumer(Box::new(consumers::LoggingConsumer));
    projector.add_consumer(Box::new(consumers::CounterStateConsumer::new()));
    projector.add_read_model(Box::new(sqlite_read_model));

    let replay = ReplayManager::new(ReplayStrategy::Checkpoint)
        .with_fallback_checkpoint(projector.checkpoint().get().await.unwrap_or(0));

    info!(
        "Projector worker running (poll interval: {:?}, batch size: {}, checkpoint: {}, store_backend: {})",
        config.poll_interval(),
        config.batch_size,
        config.checkpoint_path,
        checkpoint_backend.as_str(),
    );

    replay_outbox(
        &source,
        &projector,
        &state,
        replay.start_sequence(),
        config.batch_size,
    )
    .await?;

    if let Some(nats_url) = &config.nats_url {
        let queue_group =
            (!config.nats_queue_group.is_empty()).then_some(config.nats_queue_group.as_str());
        info!(subject = %config.nats_subject, queue_group = %config.nats_queue_group, "projector switching to live NATS tail");
        let mut live =
            LiveEventSubscriber::connect(nats_url, &config.nats_subject, queue_group).await?;

        loop {
            tokio::select! {
                _ = shutdown_signal() => {
                    state.set_healthy(false).await;
                    info!("shutdown signal received, stopping projector worker");
                    return Ok(());
                }
                result = live.try_next(config.poll_interval()) => {
                    if let Some(envelope) = result? {
                        let projected = projector.process_event(&envelope).await?;
                        state.record_count("projected_count", projected).await;
                    }
                }
            }
        }
    }

    loop {
        tokio::select! {
            _ = shutdown_signal() => {
                state.set_healthy(false).await;
                info!("shutdown signal received, stopping projector worker");
                break;
            }
            _ = tokio::time::sleep(config.poll_interval()) => {}
        }

        let events = source
            .fetch_since(
                projector.checkpoint().get().await.unwrap_or(0),
                config.batch_size,
            )
            .await?;

        if !events.is_empty() {
            let mut projected = 0;
            for event in events {
                projected += projector.process_event(&event.envelope).await?;
                let current = projector.checkpoint().get().await.unwrap_or(0);
                if event.sequence > current {
                    projector
                        .checkpoint()
                        .advance(event.sequence)
                        .await
                        .map_err(|error| ProjectorError::Checkpoint(error.to_string()))?;
                }
            }
            state.record_count("projected_count", projected).await;
        }
    }

    Ok(())
}

async fn replay_outbox(
    source: &CounterOutboxSource<TursoBackend>,
    projector: &Projector,
    state: &WorkerHealthState,
    start_sequence: u64,
    batch_size: usize,
) -> Result<(), ProjectorError> {
    let mut since = start_sequence;

    loop {
        let events = source.fetch_since(since, batch_size).await?;
        if events.is_empty() {
            return Ok(());
        }

        let mut projected = 0;
        for event in events {
            projected += projector.process_event(&event.envelope).await?;
            let current = projector.checkpoint().get().await.unwrap_or(0);
            if event.sequence > current {
                projector
                    .checkpoint()
                    .advance(event.sequence)
                    .await
                    .map_err(|error| ProjectorError::Checkpoint(error.to_string()))?;
            }
            since = event.sequence;
        }
        state.record_count("projected_count", projected).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use contracts_events::{AppEvent, CounterChanged, CounterOperation};
    use data::ports::lib_sql::LibSqlPort;
    use event_bus::outbox::OUTBOX_TABLE_SQL;
    use event_bus::ports::EventEnvelope;
    use serde::Deserialize;
    use worker_runtime::FileCheckpointStore;

    #[derive(Debug, Deserialize)]
    struct ProjectionRow {
        tenant_id: String,
        counter_key: String,
        value: i64,
        version: i64,
        operation: String,
    }

    async fn insert_counter_event(
        db: &TursoBackend,
        tenant_id: &str,
        counter_key: &str,
        operation: CounterOperation,
        new_value: i64,
        delta: i64,
        version: i64,
    ) -> u64 {
        let event = AppEvent::CounterChanged(CounterChanged {
            tenant_id: tenant_id.to_string(),
            counter_key: counter_key.to_string(),
            operation,
            new_value,
            delta,
            version,
        });
        let envelope = EventEnvelope::new(event, "counter-service");
        let payload = serde_json::to_string(&envelope).unwrap();
        db.execute(
            "INSERT INTO event_outbox (event_id, event_type, event_payload, source_service, correlation_id, status) \
             VALUES (?, ?, ?, ?, ?, 'pending')",
            vec![
                envelope.id.to_string(),
                envelope.metadata.event_type,
                payload,
                envelope.source_service,
                "projection-rebuild-test".to_string(),
            ],
        )
        .await
        .unwrap();

        let rows: Vec<serde_json::Value> = db
            .query("SELECT max(sequence) AS sequence FROM event_outbox", vec![])
            .await
            .unwrap();
        rows[0]["sequence"].as_i64().unwrap() as u64
    }

    async fn projection_row(db: &TursoBackend) -> ProjectionRow {
        let rows: Vec<ProjectionRow> = db
            .query(
                "SELECT tenant_id, counter_key, value, version, operation FROM counter_projection",
                vec![],
            )
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
        rows.into_iter().next().unwrap()
    }

    #[tokio::test]
    async fn rebuilds_counter_projection_from_outbox_history() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("projector-rebuild.db");
        let db = TursoBackend::connect(db_path.to_str().unwrap(), None)
            .await
            .unwrap();
        db.execute_batch(OUTBOX_TABLE_SQL).await.unwrap();

        insert_counter_event(
            &db,
            "tenant-a",
            "counter-a",
            CounterOperation::Increment,
            1,
            1,
            1,
        )
        .await;
        insert_counter_event(
            &db,
            "tenant-a",
            "counter-a",
            CounterOperation::Increment,
            2,
            1,
            2,
        )
        .await;
        let last_sequence = insert_counter_event(
            &db,
            "tenant-a",
            "counter-a",
            CounterOperation::Decrement,
            1,
            -1,
            3,
        )
        .await;

        let read_model = SqliteCounterReadModel::new(db.clone());
        read_model.init().await.unwrap();
        let checkpoint_path = dir.path().join("projector-checkpoint.json");
        let mut projector = Projector::with_checkpoint(Box::new(FileCheckpointStore::new(
            checkpoint_path.to_str().unwrap(),
            0,
        )));
        projector.add_consumer(Box::new(consumers::CounterStateConsumer::new()));
        projector.add_read_model(Box::new(read_model));

        let source = CounterOutboxSource::new(db.clone());
        let state = WorkerHealthState::default();
        replay_outbox(&source, &projector, &state, 0, 2)
            .await
            .unwrap();

        let row = projection_row(&db).await;
        assert_eq!(row.tenant_id, "tenant-a");
        assert_eq!(row.counter_key, "counter-a");
        assert_eq!(row.value, 1);
        assert_eq!(row.version, 3);
        assert_eq!(row.operation, "decrement");
        assert_eq!(projector.checkpoint().get().await.unwrap(), last_sequence);

        db.execute("DELETE FROM counter_projection", vec![])
            .await
            .unwrap();
        let rebuilt_read_model = SqliteCounterReadModel::new(db.clone());
        rebuilt_read_model.init().await.unwrap();
        let mut rebuilt_projector = Projector::with_checkpoint(Box::new(FileCheckpointStore::new(
            checkpoint_path.to_str().unwrap(),
            0,
        )));
        rebuilt_projector.add_consumer(Box::new(consumers::CounterStateConsumer::new()));
        rebuilt_projector.add_read_model(Box::new(rebuilt_read_model));

        replay_outbox(&source, &rebuilt_projector, &state, 0, 2)
            .await
            .unwrap();
        let rebuilt = projection_row(&db).await;
        assert_eq!(rebuilt.value, row.value);
        assert_eq!(rebuilt.version, row.version);
        assert_eq!(rebuilt.operation, row.operation);
    }
}
