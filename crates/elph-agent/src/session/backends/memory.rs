//! In-memory session storage backend.

use std::sync::Arc;

use tokio::sync::Mutex;

use crate::session::id::{generate_entry_id, generate_session_id};
use crate::session::storage_utils::{
    append_to_index, build_index, compute_statistics, create_leaf_entry, find_entries, get_entries_cursor,
    get_path_to_root, get_path_to_root_or_compaction,
};
use crate::session::types::{
    CheckpointTail, CursorPosition, SessionError, SessionErrorCode, SessionIndex, SessionMetadata, SessionStatistics,
    SessionStorage, SessionTreeEntry,
};

#[derive(Clone)]
pub struct InMemorySessionStorage {
    metadata: SessionMetadata,
    index: Arc<Mutex<SessionIndex>>,
}

impl InMemorySessionStorage {
    pub fn new(options: Option<InMemorySessionOptions>) -> Result<Self, SessionError> {
        let options = options.unwrap_or_default();
        let index = build_index(options.entries.unwrap_or_default(), options.leaf_id)?;
        let metadata = options.metadata.unwrap_or_else(|| SessionMetadata {
            id: generate_session_id(),
            created_at: crate::messages::now_iso_timestamp(),
        });
        Ok(Self {
            metadata,
            index: Arc::new(Mutex::new(index)),
        })
    }
}

#[derive(Debug, Clone, Default)]
pub struct InMemorySessionOptions {
    pub entries: Option<Vec<SessionTreeEntry>>,
    pub leaf_id: Option<String>,
    pub metadata: Option<SessionMetadata>,
}

impl SessionStorage for InMemorySessionStorage {
    type Metadata = SessionMetadata;

    async fn get_metadata(&self) -> Self::Metadata {
        self.metadata.clone()
    }

    async fn get_leaf_id(&self) -> Result<Option<String>, SessionError> {
        let index = self.index.lock().await;
        if let Some(leaf_id) = &index.leaf_id
            && !index.by_id.contains_key(leaf_id)
        {
            // Phantom leaf (crash between leaf-write and child write, rows pruned).
            return Ok(None);
        }
        Ok(index.leaf_id.clone())
    }

    async fn set_leaf_id(&mut self, leaf_id: Option<String>) -> Result<(), SessionError> {
        let mut index = self.index.lock().await;
        if let Some(leaf_id) = &leaf_id
            && !index.by_id.contains_key(leaf_id)
        {
            return Err(SessionError::new(
                SessionErrorCode::NotFound,
                format!("Entry {leaf_id} not found"),
            ));
        }
        let entry = create_leaf_entry(index.leaf_id.clone(), leaf_id.clone(), &index.by_id);
        append_to_index(&mut index, entry);
        Ok(())
    }

    async fn create_entry_id(&self) -> String {
        let index = self.index.lock().await;
        generate_entry_id(&index.by_id)
    }

    async fn append_entry(&mut self, entry: SessionTreeEntry) -> Result<(), SessionError> {
        let mut index = self.index.lock().await;
        append_to_index(&mut index, entry);
        Ok(())
    }

    async fn get_entry(&self, id: &str) -> Option<SessionTreeEntry> {
        let index = self.index.lock().await;
        index.by_id.get(id).cloned()
    }

    async fn find_entries(&self, entry_type: &str) -> Vec<SessionTreeEntry> {
        let index = self.index.lock().await;
        find_entries(&index.entries, entry_type)
    }

    async fn get_label(&self, id: &str) -> Option<String> {
        let index = self.index.lock().await;
        index.labels_by_id.get(id).cloned()
    }

    async fn get_path_to_root(&self, leaf_id: Option<&str>) -> Result<Vec<SessionTreeEntry>, SessionError> {
        let index = self.index.lock().await;
        get_path_to_root(&index.by_id, leaf_id)
    }

    async fn get_path_to_root_or_compaction(
        &self,
        leaf_id: Option<&str>,
    ) -> Result<Vec<SessionTreeEntry>, SessionError> {
        let index = self.index.lock().await;
        get_path_to_root_or_compaction(&index.by_id, leaf_id)
    }

    async fn get_entries(&self) -> Vec<SessionTreeEntry> {
        let index = self.index.lock().await;
        index.entries.clone()
    }

    async fn get_entries_cursor(&self, cursor: &CursorPosition) -> Result<Vec<SessionTreeEntry>, SessionError> {
        let index = self.index.lock().await;
        get_entries_cursor(&index.entries, cursor)
    }

    async fn get_statistics(&self) -> SessionStatistics {
        let index = self.index.lock().await;
        compute_statistics(&index)
    }

    async fn store_checkpoint_tail(&mut self, tail: CheckpointTail) -> Result<String, SessionError> {
        let mut index = self.index.lock().await;
        let root_id = tail.root_id.clone();
        index.checkpoints.insert(root_id.clone(), tail);
        Ok(root_id)
    }

    async fn load_checkpoint_tail(&self, root_id: &str) -> Result<Option<CheckpointTail>, SessionError> {
        let index = self.index.lock().await;
        Ok(index.checkpoints.get(root_id).cloned())
    }

    async fn list_checkpoint_tails(&self) -> Vec<String> {
        let index = self.index.lock().await;
        index.checkpoints.keys().cloned().collect()
    }

    async fn physical_prune_except(&mut self, keep_ids: &[String]) -> Result<usize, SessionError> {
        let keep: std::collections::HashSet<&str> = keep_ids.iter().map(String::as_str).collect();
        let index = self.index.lock().await;
        let before = index.entries.len();
        let remaining: Vec<SessionTreeEntry> = index
            .entries
            .iter()
            .filter(|e| keep.contains(e.id()))
            .cloned()
            .collect();
        let leaf = index.leaf_id.clone().filter(|id| keep.contains(id.as_str()));
        drop(index);
        let new_index = build_index(remaining, leaf)?;
        let deleted = before.saturating_sub(new_index.entries.len());
        *self.index.lock().await = new_index;
        Ok(deleted)
    }

    async fn get_name(&self) -> Option<String> {
        let index = self.index.lock().await;
        index.name.clone()
    }
}
