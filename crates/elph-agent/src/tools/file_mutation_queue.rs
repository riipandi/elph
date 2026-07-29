//! File mutation queue — serializes multiple file mutations to avoid conflicts.
//!
//! Ported from pi-agent-core's `harness/tools/file-mutation-queue.ts`.
//! Queues file write/edit/delete operations and executes them sequentially.

use std::sync::Arc;

use parking_lot::Mutex;

use crate::runtime::local_env::LocalExecutionEnv;

/// Status of a queued file mutation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MutationStatus {
    Pending,
    Applied,
    Failed,
    RolledBack,
}

/// A single file mutation operation.
#[derive(Debug, Clone)]
pub struct FileMutation {
    pub id: String,
    pub path: String,
    pub kind: MutationKind,
    pub status: MutationStatus,
    pub error: Option<String>,
}

/// Types of file mutations.
#[derive(Debug, Clone)]
pub enum MutationKind {
    Write { content: String },
    Edit { old_string: String, new_string: String },
    Delete,
    CreateDir,
    Copy { destination: String },
    Move { destination: String },
}

/// File mutation queue for serializing mutations.
///
/// Queues file write/edit/delete operations and executes them sequentially,
/// ensuring that concurrent mutations don't conflict.
#[derive(Clone)]
pub struct FileMutationQueue {
    #[allow(dead_code)]
    env: Arc<LocalExecutionEnv>,
    mutations: Arc<Mutex<Vec<FileMutation>>>,
    applied: Arc<Mutex<Vec<FileMutation>>>,
}

impl FileMutationQueue {
    pub fn new(env: Arc<LocalExecutionEnv>) -> Self {
        Self {
            env,
            mutations: Arc::new(Mutex::new(Vec::new())),
            applied: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn queue(&self, mutation: FileMutation) {
        self.mutations.lock().push(mutation);
    }

    pub fn pending(&self) -> Vec<FileMutation> {
        self.mutations
            .lock()
            .iter()
            .filter(|m| m.status == MutationStatus::Pending)
            .cloned()
            .collect()
    }

    pub fn applied(&self) -> Vec<FileMutation> {
        self.applied.lock().clone()
    }

    /// Apply all queued mutations sequentially.
    /// Stops on first failure.
    pub async fn apply_all(&self) -> Result<Vec<FileMutation>, String> {
        let mut results = Vec::new();
        let mutations = self.mutations.lock().drain(..).collect::<Vec<_>>();

        for mut mutation in mutations {
            match self.apply_one(&mutation).await {
                Ok(()) => {
                    mutation.status = MutationStatus::Applied;
                    self.applied.lock().push(mutation.clone());
                    results.push(mutation);
                }
                Err(error) => {
                    mutation.status = MutationStatus::Failed;
                    mutation.error = Some(error.clone());
                    results.push(mutation);
                    return Err(error);
                }
            }
        }

        Ok(results)
    }

    /// Rollback all applied mutations in reverse order (best-effort).
    pub async fn rollback_all(&self) {
        let applied = self.applied.lock().drain(..).rev().collect::<Vec<_>>();

        for mut mutation in applied {
            match &mutation.kind {
                MutationKind::Copy { destination } => {
                    tokio::fs::remove_file(destination).await.ok();
                }
                MutationKind::Move { destination } => {
                    tokio::fs::rename(destination, &mutation.path).await.ok();
                }
                _ => {}
            }
            mutation.status = MutationStatus::RolledBack;
        }
    }

    async fn apply_one(&self, mutation: &FileMutation) -> Result<(), String> {
        match &mutation.kind {
            MutationKind::Write { content } => tokio::fs::write(&mutation.path, content)
                .await
                .map_err(|e| format!("write failed: {e}")),
            MutationKind::Edit { old_string, new_string } => {
                let content = tokio::fs::read_to_string(&mutation.path)
                    .await
                    .map_err(|e| format!("read failed: {e}"))?;
                if !content.contains(old_string.as_str()) {
                    return Err(format!("old_string not found in {}", mutation.path));
                }
                let new_content = content.replace(old_string.as_str(), new_string.as_str());
                tokio::fs::write(&mutation.path, &new_content)
                    .await
                    .map_err(|e| format!("edit failed: {e}"))
            }
            MutationKind::Delete => tokio::fs::remove_file(&mutation.path)
                .await
                .map_err(|e| format!("delete failed: {e}")),
            MutationKind::CreateDir => tokio::fs::create_dir_all(&mutation.path)
                .await
                .map_err(|e| format!("create_dir failed: {e}")),
            MutationKind::Copy { destination } => {
                tokio::fs::copy(&mutation.path, destination)
                    .await
                    .map_err(|e| format!("copy failed: {e}"))?;
                Ok(())
            }
            MutationKind::Move { destination } => tokio::fs::rename(&mutation.path, destination)
                .await
                .map_err(|e| format!("move failed: {e}")),
        }
    }
}
