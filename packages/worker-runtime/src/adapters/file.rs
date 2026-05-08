//! Local fallback implementations for single-instance development workers.

use async_trait::async_trait;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::sync::RwLock;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::checkpoint::{CheckpointStore, CheckpointStoreError};
use crate::dedupe::{DedupeStore, DedupeStoreError};
use crate::idempotency::{IdempotencyStatus, IdempotencyStore, IdempotencyStoreError};

// ── File Checkpoint Store ────────────────────────────────────

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct CheckpointState {
    last_processed: u64,
}

/// File-based checkpoint store.
pub struct FileCheckpointStore {
    last_processed: AtomicU64,
    checkpoint_path: String,
}

impl FileCheckpointStore {
    pub fn new(checkpoint_path: &str, initial: u64) -> Self {
        let loaded = Self::load_from_disk(checkpoint_path).unwrap_or(initial);
        Self {
            last_processed: AtomicU64::new(loaded),
            checkpoint_path: checkpoint_path.to_string(),
        }
    }

    fn load_from_disk(path: &str) -> Option<u64> {
        let path_ref = Path::new(path);
        if !path_ref.exists() {
            return None;
        }

        match fs::read_to_string(path_ref) {
            Ok(content) => match serde_json::from_str::<CheckpointState>(&content) {
                Ok(state) => Some(state.last_processed),
                Err(e) => {
                    tracing::warn!(error = %e, "failed to parse checkpoint file");
                    None
                }
            },
            Err(e) => {
                tracing::warn!(error = %e, "failed to read checkpoint file");
                None
            }
        }
    }

    fn persist(&self) -> Result<(), CheckpointStoreError> {
        let state = CheckpointState {
            last_processed: self.last_processed.load(Ordering::Acquire),
        };
        serde_json::to_string_pretty(&state)
            .map_err(|e| CheckpointStoreError::PersistFailed(e.to_string()))
            .and_then(|json| {
                fs::write(&self.checkpoint_path, json)
                    .map_err(|e| CheckpointStoreError::PersistFailed(e.to_string()))
            })
    }
}

#[async_trait]
impl CheckpointStore for FileCheckpointStore {
    async fn get(&self) -> Result<u64, CheckpointStoreError> {
        Ok(self.last_processed.load(Ordering::Acquire))
    }

    async fn advance(&self, new_value: u64) -> Result<(), CheckpointStoreError> {
        self.last_processed.store(new_value, Ordering::Release);
        self.persist()
    }
}

// ── File Idempotency Store ───────────────────────────────────

/// Local fallback idempotency store.
///
/// Despite the historical name, this implementation is in-memory and intended
/// only for single-instance local development.
pub struct FileIdempotencyStore {
    store: RwLock<HashMap<String, IdempotencyStatus>>,
    max_size: usize,
}

impl FileIdempotencyStore {
    pub fn new(max_size: usize) -> Self {
        Self {
            store: RwLock::new(HashMap::with_capacity(max_size)),
            max_size,
        }
    }

    fn evict_if_needed(&self, store: &mut HashMap<String, IdempotencyStatus>) {
        if store.len() >= self.max_size {
            let to_remove: Vec<String> = store
                .iter()
                .filter(|(_, v)| {
                    matches!(
                        v,
                        IdempotencyStatus::Completed | IdempotencyStatus::Failed(_)
                    )
                })
                .take(store.len() / 4)
                .map(|(k, _)| k.clone())
                .collect();
            for k in to_remove {
                store.remove(&k);
            }
        }
    }
}

#[async_trait]
impl IdempotencyStore for FileIdempotencyStore {
    async fn check(&self, key: &str) -> Result<IdempotencyStatus, IdempotencyStoreError> {
        let store = self.store.read().unwrap();
        Ok(store
            .get(key)
            .cloned()
            .unwrap_or(IdempotencyStatus::Unknown))
    }

    async fn start(&self, key: &str) -> Result<bool, IdempotencyStoreError> {
        let mut store = self.store.write().unwrap();
        self.evict_if_needed(&mut store);

        match store.get(key) {
            Some(IdempotencyStatus::InProgress) | Some(IdempotencyStatus::Completed) => Ok(false),
            _ => {
                store.insert(key.to_string(), IdempotencyStatus::InProgress);
                Ok(true)
            }
        }
    }

    async fn complete(&self, key: &str) -> Result<(), IdempotencyStoreError> {
        let mut store = self.store.write().unwrap();
        store.insert(key.to_string(), IdempotencyStatus::Completed);
        Ok(())
    }

    async fn fail(&self, key: &str, error: String) -> Result<(), IdempotencyStoreError> {
        let mut store = self.store.write().unwrap();
        store.insert(key.to_string(), IdempotencyStatus::Failed(error));
        Ok(())
    }

    async fn is_already_processed(&self, key: &str) -> Result<bool, IdempotencyStoreError> {
        let store = self.store.read().unwrap();
        Ok(matches!(store.get(key), Some(IdempotencyStatus::Completed)))
    }
}

impl Default for FileIdempotencyStore {
    fn default() -> Self {
        Self::new(50_000)
    }
}

// ── File Dedupe Store ────────────────────────────────────────

/// Local fallback deduplication store.
///
/// Despite the historical name, this implementation is in-memory and intended
/// only for single-instance local development.
pub struct FileDedupeStore {
    seen: RwLock<HashSet<String>>,
    max_size: usize,
}

impl FileDedupeStore {
    pub fn new(max_size: usize) -> Self {
        Self {
            seen: RwLock::new(HashSet::with_capacity(max_size)),
            max_size,
        }
    }

    fn evict_if_needed(&self, seen: &mut HashSet<String>) {
        if seen.len() >= self.max_size {
            let to_remove: Vec<String> = seen.iter().take(seen.len() / 4).cloned().collect();
            for key in to_remove {
                seen.remove(&key);
            }
        }
    }
}

#[async_trait]
impl DedupeStore for FileDedupeStore {
    async fn is_duplicate(&self, id: &str) -> Result<bool, DedupeStoreError> {
        let seen = self.seen.read().unwrap();
        Ok(seen.contains(id))
    }

    async fn mark_processed(&self, id: &str) -> Result<bool, DedupeStoreError> {
        let mut seen = self.seen.write().unwrap();
        self.evict_if_needed(&mut seen);
        Ok(seen.insert(id.to_string()))
    }
}

impl Default for FileDedupeStore {
    fn default() -> Self {
        Self::new(10_000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn checkpoint_corruption_falls_back_to_initial_value() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("checkpoint.json");
        std::fs::write(&path, "not valid json").unwrap();

        let store = FileCheckpointStore::new(path.to_str().unwrap(), 7);

        assert_eq!(store.get().await.unwrap(), 7);
    }
}
