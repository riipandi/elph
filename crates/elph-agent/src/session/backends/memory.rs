//! In-memory session storage backend.

use async_trait::async_trait;

use crate::session::id::{generate_entry_id, generate_session_id};
use crate::session::storage_utils::{append_to_index, build_index, create_leaf_entry, find_entries, get_path_to_root};
use crate::session::types::{
    SessionError, SessionErrorCode, SessionIndex, SessionMetadata, SessionStorage, SessionTreeEntry,
};

#[derive(Clone)]
pub struct InMemorySessionStorage {
    metadata: SessionMetadata,
    index: SessionIndex,
}

impl InMemorySessionStorage {
    pub fn new(options: Option<InMemorySessionOptions>) -> Result<Self, SessionError> {
        let options = options.unwrap_or_default();
        let index = build_index(options.entries.unwrap_or_default(), options.leaf_id)?;
        let metadata = options.metadata.unwrap_or_else(|| SessionMetadata {
            id: generate_session_id(),
            created_at: crate::messages::now_iso_timestamp(),
        });
        Ok(Self { metadata, index })
    }
}

#[derive(Debug, Clone, Default)]
pub struct InMemorySessionOptions {
    pub entries: Option<Vec<SessionTreeEntry>>,
    pub leaf_id: Option<String>,
    pub metadata: Option<SessionMetadata>,
}

#[async_trait]
impl SessionStorage for InMemorySessionStorage {
    type Metadata = SessionMetadata;

    async fn get_metadata(&self) -> Self::Metadata {
        self.metadata.clone()
    }

    async fn get_leaf_id(&self) -> Result<Option<String>, SessionError> {
        if let Some(leaf_id) = &self.index.leaf_id
            && !self.index.by_id.contains_key(leaf_id)
        {
            return Err(SessionError::new(
                SessionErrorCode::InvalidSession,
                format!("Entry {leaf_id} not found"),
            ));
        }
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
        append_to_index(&mut self.index, entry);
        Ok(())
    }

    async fn create_entry_id(&self) -> String {
        generate_entry_id(&self.index.by_id)
    }

    async fn append_entry(&mut self, entry: SessionTreeEntry) -> Result<(), SessionError> {
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

    async fn get_entries(&self) -> Vec<SessionTreeEntry> {
        self.index.entries.clone()
    }
}
