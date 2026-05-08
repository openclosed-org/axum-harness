//! Outbox poller — queries the database for pending outbox entries.
//!
//! Provides both in-memory (testing) and libsql-backed (production) readers.
//! Both read from the unified `event_outbox` table.

use std::time::Duration;

use async_trait::async_trait;
use tokio::time;
use tracing::{debug, warn};

use crate::checkpoint::CheckpointStorePort;
use crate::dedupe::DedupeStorePort;
use data::ports::lib_sql::LibSqlPort;
use serde::Deserialize;

/// Represents a pending outbox entry.
#[derive(Debug, Clone)]
pub struct PendingOutboxEntry {
    pub id: String,
    pub sequence: u64,
    pub event_type: String,
    pub payload: String,
    pub source_service: String,
    pub correlation_id: Option<String>,
    pub retry_count: u32,
}

/// Abstract port for reading pending outbox entries.
#[async_trait]
pub trait OutboxReader: Send + Sync {
    /// Fetch pending outbox entries since the given checkpoint.
    async fn fetch_pending(
        &self,
        since_sequence: u64,
        limit: usize,
    ) -> Result<Vec<PendingOutboxEntry>, Box<dyn std::error::Error + Send + Sync>>;

    /// Mark outbox entries as published.
    /// Default is a no-op (in-memory readers don't need this).
    async fn mark_published(
        &self,
        _entry_ids: &[String],
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }
}

/// In-memory stub reader for testing.
pub struct MemoryOutboxReader {
    pub entries: Vec<PendingOutboxEntry>,
}

impl MemoryOutboxReader {
    pub fn new(entries: Vec<PendingOutboxEntry>) -> Self {
        Self { entries }
    }
}

/// Row shape from the unified event_outbox table.
#[derive(Debug, Deserialize)]
struct OutboxRow {
    sequence: i64,
    event_id: String,
    event_type: String,
    event_payload: String,
    source_service: String,
    correlation_id: Option<String>,
    retry_count: i64,
}

/// LibSQL-backed outbox reader for production use.
///
/// Reads from the unified `event_outbox` table where `status` is pending or failed,
/// ordered by `sequence ASC` (FIFO with checkpoint support).
pub struct LibSqlOutboxReader<P: LibSqlPort> {
    port: P,
}

impl<P: LibSqlPort> LibSqlOutboxReader<P> {
    pub fn new(port: P) -> Self {
        Self { port }
    }
}

#[async_trait]
impl<P: LibSqlPort> OutboxReader for LibSqlOutboxReader<P> {
    async fn fetch_pending(
        &self,
        since_sequence: u64,
        limit: usize,
    ) -> Result<Vec<PendingOutboxEntry>, Box<dyn std::error::Error + Send + Sync>> {
        let rows: Vec<OutboxRow> = self
            .port
            .query(
                "SELECT sequence, event_id, event_type, event_payload, source_service, correlation_id, retry_count \
                 FROM event_outbox \
                 WHERE status IN ('pending', 'failed') AND sequence > ? AND retry_count < 5 \
                 ORDER BY sequence ASC \
                 LIMIT ?",
                vec![since_sequence.to_string(), limit.to_string()],
            )
            .await?;

        let entries = rows
            .into_iter()
            .map(|r| PendingOutboxEntry {
                id: r.event_id,
                sequence: r.sequence as u64,
                event_type: r.event_type,
                payload: r.event_payload,
                source_service: r.source_service,
                correlation_id: r.correlation_id.filter(|value| !value.is_empty()),
                retry_count: r.retry_count as u32,
            })
            .collect();

        Ok(entries)
    }

    async fn mark_published(
        &self,
        entry_ids: &[String],
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if entry_ids.is_empty() {
            return Ok(());
        }

        let placeholders: Vec<String> = entry_ids.iter().map(|_| "?".to_string()).collect();
        let sql = format!(
            "UPDATE event_outbox SET status = 'published', published_at = datetime('now') WHERE event_id IN ({})",
            placeholders.join(", ")
        );

        self.port.execute(&sql, entry_ids.to_vec()).await?;
        Ok(())
    }
}

#[async_trait]
impl OutboxReader for MemoryOutboxReader {
    async fn fetch_pending(
        &self,
        since_sequence: u64,
        _limit: usize,
    ) -> Result<Vec<PendingOutboxEntry>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(self
            .entries
            .iter()
            .filter(|e| e.sequence > since_sequence)
            .cloned()
            .collect())
    }
}

/// Implement OutboxReader for boxed trait objects
#[async_trait]
impl OutboxReader for Box<dyn OutboxReader> {
    async fn fetch_pending(
        &self,
        since_sequence: u64,
        limit: usize,
    ) -> Result<Vec<PendingOutboxEntry>, Box<dyn std::error::Error + Send + Sync>> {
        self.as_ref().fetch_pending(since_sequence, limit).await
    }

    async fn mark_published(
        &self,
        entry_ids: &[String],
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.as_ref().mark_published(entry_ids).await
    }
}

/// Configuration for the outbox poller.
#[derive(Debug, Clone)]
pub struct PollerConfig {
    pub poll_interval: Duration,
    pub batch_size: usize,
}

impl Default for PollerConfig {
    fn default() -> Self {
        Self {
            poll_interval: Duration::from_secs(5),
            batch_size: 100,
        }
    }
}

/// Polls the outbox and yields pending entries to the publisher.
pub struct OutboxPoller<R: OutboxReader> {
    pub reader: R,
    config: PollerConfig,
    checkpoint: Box<dyn CheckpointStorePort>,
    dedup: Box<dyn DedupeStorePort>,
}

impl<R: OutboxReader> OutboxPoller<R> {
    pub fn new(
        reader: R,
        config: PollerConfig,
        checkpoint: Box<dyn CheckpointStorePort>,
        dedup: Box<dyn DedupeStorePort>,
    ) -> Self {
        Self {
            reader,
            config,
            checkpoint,
            dedup,
        }
    }

    /// Run one poll cycle, returning entries to process.
    pub async fn poll_cycle(&mut self) -> Vec<PendingOutboxEntry> {
        let since = match self.checkpoint.get().await {
            Ok(sequence) => sequence,
            Err(error) => {
                warn!(error = %error, "failed to load checkpoint");
                0
            }
        };

        match self
            .reader
            .fetch_pending(since, self.config.batch_size)
            .await
        {
            Ok(entries) => {
                let mut result = Vec::new();
                for entry in entries {
                    let is_duplicate = match self.dedup.is_duplicate(&entry.id).await {
                        Ok(is_duplicate) => is_duplicate,
                        Err(error) => {
                            warn!(error = %error, entry_id = %entry.id, "failed to check dedupe store");
                            false
                        }
                    };

                    if !is_duplicate {
                        result.push(entry);
                    } else {
                        debug!(entry_id = %entry.id, "skipping duplicate outbox entry");
                    }
                }

                debug!(count = result.len(), "fetched pending outbox entries");
                result
            }
            Err(e) => {
                warn!(error = %e, "failed to fetch pending outbox entries");
                Vec::new()
            }
        }
    }

    /// Mark entries as processed and advance the checkpoint.
    pub async fn mark_processed(&mut self, entries: &[PendingOutboxEntry]) {
        for entry in entries {
            if let Err(error) = self.dedup.mark_processed(&entry.id).await {
                warn!(error = %error, entry_id = %entry.id, "failed to mark dedupe entry as processed");
            }

            let current_checkpoint = match self.checkpoint.get().await {
                Ok(sequence) => sequence,
                Err(error) => {
                    warn!(error = %error, "failed to load checkpoint while advancing");
                    0
                }
            };

            if entry.sequence > current_checkpoint
                && let Err(error) = self.checkpoint.advance(entry.sequence).await
            {
                warn!(error = %error, sequence = entry.sequence, "failed to advance checkpoint");
            }
        }
    }

    pub async fn mark_processed_by_id(
        &mut self,
        entries: &[PendingOutboxEntry],
        entry_ids: &[String],
    ) {
        let completed: std::collections::HashSet<&str> =
            entry_ids.iter().map(String::as_str).collect();
        let processed: Vec<PendingOutboxEntry> = entries
            .iter()
            .filter(|entry| completed.contains(entry.id.as_str()))
            .cloned()
            .collect();
        self.mark_processed(&processed).await;
    }

    /// Mark entries as published in the database and advance checkpoint.
    pub async fn mark_published(
        &mut self,
        entry_ids: &[String],
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.reader.mark_published(entry_ids).await
    }

    /// Run the poller loop (for integration into a larger worker).
    pub async fn run<F, Fut>(&mut self, mut handler: F)
    where
        F: FnMut(Vec<PendingOutboxEntry>) -> Fut + Send,
        Fut: std::future::Future<Output = ()> + Send,
    {
        let mut interval = time::interval(self.config.poll_interval);
        loop {
            interval.tick().await;
            let entries = self.poll_cycle().await;
            if !entries.is_empty() {
                handler(entries.clone()).await;
                self.mark_processed(&entries).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_checkpoint_path() -> &'static str {
        "/tmp/outbox-poller-test-checkpoint.json"
    }

    #[tokio::test]
    async fn poll_cycle_returns_pending_entries() {
        let _ = std::fs::remove_file(test_checkpoint_path());
        let entries = vec![PendingOutboxEntry {
            id: "entry-1".to_string(),
            sequence: 1,
            event_type: "counter.changed".to_string(),
            payload: "{}".to_string(),
            source_service: "counter-service".to_string(),
            correlation_id: None,
            retry_count: 0,
        }];
        let reader = MemoryOutboxReader::new(entries);
        let mut poller = OutboxPoller::new(
            reader,
            PollerConfig::default(),
            Box::new(worker_runtime::FileCheckpointStore::new(
                test_checkpoint_path(),
                0,
            )),
            Box::<worker_runtime::FileDedupeStore>::default(),
        );

        let result = poller.poll_cycle().await;
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, "entry-1");
        let _ = std::fs::remove_file(test_checkpoint_path());
    }

    #[tokio::test]
    async fn poll_cycle_skips_duplicates() {
        let _ = std::fs::remove_file(test_checkpoint_path());
        let entries = vec![PendingOutboxEntry {
            id: "entry-1".to_string(),
            sequence: 1,
            event_type: "counter.changed".to_string(),
            payload: "{}".to_string(),
            source_service: "counter-service".to_string(),
            correlation_id: None,
            retry_count: 0,
        }];
        let reader = MemoryOutboxReader::new(entries.clone());
        let mut poller = OutboxPoller::new(
            reader,
            PollerConfig::default(),
            Box::new(worker_runtime::FileCheckpointStore::new(
                test_checkpoint_path(),
                0,
            )),
            Box::<worker_runtime::FileDedupeStore>::default(),
        );

        // First poll
        let result1 = poller.poll_cycle().await;
        assert_eq!(result1.len(), 1);
        poller.mark_processed(&result1).await;

        // Second poll — should skip the duplicate
        let reader2 = MemoryOutboxReader::new(entries);
        poller.reader = reader2;
        let result2 = poller.poll_cycle().await;
        assert_eq!(result2.len(), 0);
        let _ = std::fs::remove_file(test_checkpoint_path());
    }

    #[tokio::test]
    async fn mark_processed_by_id_does_not_checkpoint_failed_entries() {
        let _ = std::fs::remove_file(test_checkpoint_path());
        let entries = vec![
            PendingOutboxEntry {
                id: "entry-1".to_string(),
                sequence: 1,
                event_type: "counter.changed".to_string(),
                payload: "{}".to_string(),
                source_service: "counter-service".to_string(),
                correlation_id: None,
                retry_count: 0,
            },
            PendingOutboxEntry {
                id: "entry-2".to_string(),
                sequence: 2,
                event_type: "counter.changed".to_string(),
                payload: "{}".to_string(),
                source_service: "counter-service".to_string(),
                correlation_id: None,
                retry_count: 0,
            },
        ];
        let reader = MemoryOutboxReader::new(entries.clone());
        let mut poller = OutboxPoller::new(
            reader,
            PollerConfig::default(),
            Box::new(worker_runtime::FileCheckpointStore::new(
                test_checkpoint_path(),
                0,
            )),
            Box::<worker_runtime::FileDedupeStore>::default(),
        );

        poller
            .mark_processed_by_id(&entries, &["entry-1".to_string()])
            .await;

        let result = poller.poll_cycle().await;
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, "entry-2");
        let _ = std::fs::remove_file(test_checkpoint_path());
    }
}
