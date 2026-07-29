//! JSONL-backed session storage backend.
//!
//! Ported from pi-agent-core's `JsonlSessionStorage`.
//! Stores session entries as newline-delimited JSON in a single file.

use std::path::{Path, PathBuf};

use tokio::fs::OpenOptions;
use tokio::fs::{self};
use tokio::io::AsyncWriteExt;

use crate::session::id::{generate_entry_id, generate_session_id};
use crate::session::storage_utils::{
    append_to_index, build_index, compute_statistics, create_leaf_entry, find_entries, get_entries_cursor,
    get_path_to_root, get_path_to_root_or_compaction,
};
use crate::session::types::*;

/// JSONL session storage backend.
///
/// Stores all session tree entries as newline-delimited JSON in a single file.
/// The entire file is loaded into memory on open; entries are appended to the
/// file on write.
#[derive(Clone)]
pub struct JsonlSessionStorage {
    file_path: PathBuf,
    metadata: SessionMetadata,
    index: SessionIndex,
}

impl JsonlSessionStorage {
    pub async fn open(file_path: impl AsRef<Path>) -> Result<Self, SessionError> {
        let file_path = file_path.as_ref().to_path_buf();
        let entries = if file_path.exists() {
            load_entries(&file_path).await?
        } else {
            Vec::new()
        };
        let leaf_id = entries
            .iter()
            .rev()
            .find_map(crate::session::storage_utils::leaf_id_after_entry);
        let index = build_index(entries, leaf_id)?;
        let metadata = SessionMetadata {
            id: generate_session_id(),
            created_at: crate::messages::now_iso_timestamp(),
        };
        Ok(Self {
            file_path,
            metadata,
            index,
        })
    }

    pub fn file_path(&self) -> &Path {
        &self.file_path
    }
}

async fn load_entries(file_path: &Path) -> Result<Vec<SessionTreeEntry>, SessionError> {
    let content = fs::read_to_string(file_path)
        .await
        .map_err(|e| SessionError::new(SessionErrorCode::Storage, format!("failed to read JSONL: {e}")))?;
    let mut entries = Vec::new();
    for line in content.lines().filter(|l| !l.trim().is_empty()) {
        let entry: SessionTreeEntry = serde_json::from_str(line)
            .map_err(|e| SessionError::new(SessionErrorCode::InvalidEntry, format!("invalid JSONL line: {e}")))?;
        entries.push(entry);
    }
    Ok(entries)
}

impl SessionStorage for JsonlSessionStorage {
    type Metadata = SessionMetadata;

    async fn get_metadata(&self) -> Self::Metadata {
        self.metadata.clone()
    }

    async fn get_leaf_id(&self) -> Result<Option<String>, SessionError> {
        Ok(self.index.leaf_id.clone())
    }

    async fn set_leaf_id(&mut self, leaf_id: Option<String>) -> Result<(), SessionError> {
        if let Some(leaf_id) = &leaf_id
            && !self.index.by_id.contains_key(leaf_id)
        {
            return Err(SessionError::new(
                SessionErrorCode::NotFound,
                format!("Entry {leaf_id} not found"),
            ));
        }
        let entry = create_leaf_entry(self.index.leaf_id.clone(), leaf_id.clone(), &self.index.by_id);
        self.append_entry(entry).await
    }

    async fn create_entry_id(&self) -> String {
        generate_entry_id(&self.index.by_id)
    }

    async fn append_entry(&mut self, entry: SessionTreeEntry) -> Result<(), SessionError> {
        let line = serde_json::to_string(&entry)
            .map_err(|e| SessionError::new(SessionErrorCode::Storage, format!("failed to encode entry: {e}")))?;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.file_path)
            .await
            .map_err(|e| SessionError::new(SessionErrorCode::Storage, format!("failed to open JSONL: {e}")))?;
        file.write_all(format!("{line}\n").as_bytes())
            .await
            .map_err(|e| SessionError::new(SessionErrorCode::Storage, format!("failed to append JSONL: {e}")))?;
        file.flush()
            .await
            .map_err(|e| SessionError::new(SessionErrorCode::Storage, format!("failed to flush JSONL: {e}")))?;
        append_to_index(&mut self.index, entry);
        Ok(())
    }

    async fn get_entry(&self, id: &str) -> Option<SessionTreeEntry> {
        self.index.by_id.get(id).cloned()
    }

    async fn find_entries(&self, entry_type: &str) -> Vec<SessionTreeEntry> {
        find_entries(&self.index.entries, entry_type)
    }

    async fn get_label(&self, id: &str) -> Option<String> {
        self.index.labels_by_id.get(id).cloned()
    }

    async fn get_path_to_root(&self, leaf_id: Option<&str>) -> Result<Vec<SessionTreeEntry>, SessionError> {
        get_path_to_root(&self.index.by_id, leaf_id)
    }

    async fn get_path_to_root_or_compaction(
        &self,
        leaf_id: Option<&str>,
    ) -> Result<Vec<SessionTreeEntry>, SessionError> {
        get_path_to_root_or_compaction(&self.index.by_id, leaf_id)
    }

    async fn get_entries(&self) -> Vec<SessionTreeEntry> {
        self.index.entries.clone()
    }

    async fn get_entries_cursor(&self, cursor: &CursorPosition) -> Result<Vec<SessionTreeEntry>, SessionError> {
        get_entries_cursor(&self.index.entries, cursor)
    }

    async fn get_statistics(&self) -> SessionStatistics {
        compute_statistics(&self.index)
    }

    async fn store_checkpoint_tail(&mut self, tail: CheckpointTail) -> Result<String, SessionError> {
        let root_id = tail.root_id.clone();
        self.index.checkpoints.insert(root_id.clone(), tail);
        Ok(root_id)
    }

    async fn load_checkpoint_tail(&self, root_id: &str) -> Result<Option<CheckpointTail>, SessionError> {
        Ok(self.index.checkpoints.get(root_id).cloned())
    }

    async fn list_checkpoint_tails(&self) -> Vec<String> {
        self.index.checkpoints.keys().cloned().collect()
    }

    async fn get_name(&self) -> Option<String> {
        self.index.name.clone()
    }
}
