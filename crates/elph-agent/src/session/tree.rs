//! Session tree API — append entries, branch, and build context.

use crate::messages::now_iso_timestamp;
use crate::session::context::build_session_context;
use crate::session::types::{SessionContext, SessionError, SessionErrorCode, SessionStorage, SessionTreeEntry};
use crate::types::AgentMessage;

pub struct Session<S: SessionStorage> {
    storage: S,
}

impl<S: SessionStorage + Clone> Clone for Session<S> {
    fn clone(&self) -> Self {
        Self {
            storage: self.storage.clone(),
        }
    }
}

impl<S: SessionStorage> Session<S> {
    pub fn new(storage: S) -> Self {
        Self { storage }
    }

    pub async fn metadata(&self) -> S::Metadata {
        self.storage.get_metadata().await
    }

    pub fn into_storage(self) -> S {
        self.storage
    }

    pub fn storage(&self) -> &S {
        &self.storage
    }

    pub fn storage_mut(&mut self) -> &mut S {
        &mut self.storage
    }

    pub async fn leaf_id(&self) -> Result<Option<String>, SessionError> {
        self.storage.get_leaf_id().await
    }

    pub async fn entry(&self, id: &str) -> Option<SessionTreeEntry> {
        self.storage.get_entry(id).await
    }

    pub async fn entries(&self) -> Vec<SessionTreeEntry> {
        self.storage.get_entries().await
    }

    pub async fn branch(&self, from_id: Option<&str>) -> Result<Vec<SessionTreeEntry>, SessionError> {
        let leaf_id = match from_id {
            Some(id) => Some(id.to_string()),
            None => self.storage.get_leaf_id().await?,
        };
        self.storage.get_path_to_root(leaf_id.as_deref()).await
    }

    pub async fn build_context(&self) -> Result<SessionContext, SessionError> {
        Ok(build_session_context(&self.branch(None).await?))
    }

    pub async fn label(&self, id: &str) -> Option<String> {
        self.storage.get_label(id).await
    }

    pub async fn session_name(&self) -> Option<String> {
        let entries = self.storage.find_entries("session_info").await;
        entries
            .last()
            .and_then(|entry| match entry {
                SessionTreeEntry::SessionInfo { name, .. } => name.clone(),
                _ => None,
            })
            .map(|name| {
                name.replace(['\r', '\n'], " ")
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .filter(|name| !name.is_empty())
    }

    async fn append_typed_entry(&mut self, entry: SessionTreeEntry) -> Result<String, SessionError> {
        let id = entry.id().to_string();
        self.storage.append_entry(entry).await?;
        Ok(id)
    }

    pub async fn append_message(&mut self, message: AgentMessage) -> Result<String, SessionError> {
        self.append_typed_entry(SessionTreeEntry::Message {
            id: self.storage.create_entry_id().await,
            parent_id: self.storage.get_leaf_id().await?,
            timestamp: now_iso_timestamp(),
            message,
        })
        .await
    }

    pub async fn append_thinking_level_change(
        &mut self,
        thinking_level: impl Into<String>,
    ) -> Result<String, SessionError> {
        self.append_typed_entry(SessionTreeEntry::ThinkingLevelChange {
            id: self.storage.create_entry_id().await,
            parent_id: self.storage.get_leaf_id().await?,
            timestamp: now_iso_timestamp(),
            thinking_level: thinking_level.into(),
        })
        .await
    }

    pub async fn append_model_change(
        &mut self,
        provider: impl Into<String>,
        model_id: impl Into<String>,
    ) -> Result<String, SessionError> {
        self.append_typed_entry(SessionTreeEntry::ModelChange {
            id: self.storage.create_entry_id().await,
            parent_id: self.storage.get_leaf_id().await?,
            timestamp: now_iso_timestamp(),
            provider: provider.into(),
            model_id: model_id.into(),
        })
        .await
    }

    pub async fn append_collaboration_mode_change(
        &mut self,
        mode: crate::mode::CollaborationMode,
    ) -> Result<String, SessionError> {
        self.append_typed_entry(SessionTreeEntry::CollaborationModeChange {
            id: self.storage.create_entry_id().await,
            parent_id: self.storage.get_leaf_id().await?,
            timestamp: now_iso_timestamp(),
            mode,
        })
        .await
    }

    pub async fn append_active_tools_change(&mut self, active_tool_names: Vec<String>) -> Result<String, SessionError> {
        self.append_typed_entry(SessionTreeEntry::ActiveToolsChange {
            id: self.storage.create_entry_id().await,
            parent_id: self.storage.get_leaf_id().await?,
            timestamp: now_iso_timestamp(),
            active_tool_names,
        })
        .await
    }

    pub async fn append_compaction(
        &mut self,
        summary: impl Into<String>,
        first_kept_entry_id: impl Into<String>,
        tokens_before: u64,
        details: Option<serde_json::Value>,
        from_hook: Option<bool>,
    ) -> Result<String, SessionError> {
        self.append_typed_entry(SessionTreeEntry::Compaction {
            id: self.storage.create_entry_id().await,
            parent_id: self.storage.get_leaf_id().await?,
            timestamp: now_iso_timestamp(),
            summary: summary.into(),
            first_kept_entry_id: first_kept_entry_id.into(),
            tokens_before,
            details,
            from_hook,
        })
        .await
    }

    pub async fn append_custom_entry(
        &mut self,
        custom_type: impl Into<String>,
        data: Option<serde_json::Value>,
    ) -> Result<String, SessionError> {
        self.append_typed_entry(SessionTreeEntry::Custom {
            id: self.storage.create_entry_id().await,
            parent_id: self.storage.get_leaf_id().await?,
            timestamp: now_iso_timestamp(),
            custom_type: custom_type.into(),
            data,
        })
        .await
    }

    pub async fn append_custom_message_entry(
        &mut self,
        custom_type: impl Into<String>,
        content: crate::session::types::CustomMessageEntryContent,
        display: bool,
        details: Option<serde_json::Value>,
    ) -> Result<String, SessionError> {
        self.append_typed_entry(SessionTreeEntry::CustomMessage {
            id: self.storage.create_entry_id().await,
            parent_id: self.storage.get_leaf_id().await?,
            timestamp: now_iso_timestamp(),
            custom_type: custom_type.into(),
            content,
            display,
            details,
        })
        .await
    }

    pub async fn append_label(&mut self, target_id: &str, label: Option<&str>) -> Result<String, SessionError> {
        if self.storage.get_entry(target_id).await.is_none() {
            return Err(SessionError::new(
                SessionErrorCode::NotFound,
                format!("Entry {target_id} not found"),
            ));
        }
        self.append_typed_entry(SessionTreeEntry::Label {
            id: self.storage.create_entry_id().await,
            parent_id: self.storage.get_leaf_id().await?,
            timestamp: now_iso_timestamp(),
            target_id: target_id.to_string(),
            label: label.map(str::to_string),
        })
        .await
    }

    pub async fn append_session_name(&mut self, name: impl AsRef<str>) -> Result<String, SessionError> {
        let sanitized = name
            .as_ref()
            .replace(['\r', '\n'], " ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        self.append_typed_entry(SessionTreeEntry::SessionInfo {
            id: self.storage.create_entry_id().await,
            parent_id: self.storage.get_leaf_id().await?,
            timestamp: now_iso_timestamp(),
            name: Some(sanitized),
        })
        .await
    }

    pub async fn move_to(
        &mut self,
        entry_id: Option<&str>,
        summary: Option<BranchSummaryOptions>,
    ) -> Result<Option<String>, SessionError> {
        if let Some(entry_id) = entry_id
            && self.storage.get_entry(entry_id).await.is_none()
        {
            return Err(SessionError::new(
                SessionErrorCode::NotFound,
                format!("Entry {entry_id} not found"),
            ));
        }
        self.storage.set_leaf_id(entry_id.map(str::to_string)).await?;
        let Some(summary) = summary else {
            return Ok(None);
        };
        let summary_id = self
            .append_typed_entry(SessionTreeEntry::BranchSummary {
                id: self.storage.create_entry_id().await,
                parent_id: entry_id.map(str::to_string),
                timestamp: now_iso_timestamp(),
                from_id: entry_id.unwrap_or("root").to_string(),
                summary: summary.summary,
                details: summary.details,
                from_hook: summary.from_hook,
            })
            .await?;
        Ok(Some(summary_id))
    }
}

#[derive(Debug, Clone)]
pub struct BranchSummaryOptions {
    pub summary: String,
    pub details: Option<serde_json::Value>,
    pub from_hook: Option<bool>,
}
