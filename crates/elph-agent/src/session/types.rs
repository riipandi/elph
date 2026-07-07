//! Session tree entry types and storage trait.

use std::collections::HashMap;

use async_trait::async_trait;
use elph_ai::{ImageContent, TextContent};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::types::AgentMessage;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionErrorCode {
    NotFound,
    InvalidSession,
    InvalidEntry,
    Storage,
    Unknown,
}

#[derive(Debug, Clone)]
pub struct SessionError {
    pub code: SessionErrorCode,
    pub message: String,
}

impl SessionError {
    pub fn new(code: SessionErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for SessionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for SessionError {}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SessionTreeEntry {
    #[serde(rename = "message")]
    Message {
        id: String,
        #[serde(rename = "parentId", default, skip_serializing_if = "Option::is_none")]
        parent_id: Option<String>,
        timestamp: String,
        message: AgentMessage,
    },
    #[serde(rename = "thinking_level_change")]
    ThinkingLevelChange {
        id: String,
        #[serde(rename = "parentId", default, skip_serializing_if = "Option::is_none")]
        parent_id: Option<String>,
        timestamp: String,
        #[serde(rename = "thinkingLevel")]
        thinking_level: String,
    },
    #[serde(rename = "model_change")]
    ModelChange {
        id: String,
        #[serde(rename = "parentId", default, skip_serializing_if = "Option::is_none")]
        parent_id: Option<String>,
        timestamp: String,
        provider: String,
        #[serde(rename = "modelId")]
        model_id: String,
    },
    #[serde(rename = "active_tools_change")]
    ActiveToolsChange {
        id: String,
        #[serde(rename = "parentId", default, skip_serializing_if = "Option::is_none")]
        parent_id: Option<String>,
        timestamp: String,
        #[serde(rename = "activeToolNames")]
        active_tool_names: Vec<String>,
    },
    #[serde(rename = "compaction")]
    Compaction {
        id: String,
        #[serde(rename = "parentId", default, skip_serializing_if = "Option::is_none")]
        parent_id: Option<String>,
        timestamp: String,
        summary: String,
        #[serde(rename = "firstKeptEntryId")]
        first_kept_entry_id: String,
        #[serde(rename = "tokensBefore")]
        tokens_before: u64,
        #[serde(skip_serializing_if = "Option::is_none")]
        details: Option<Value>,
        #[serde(skip_serializing_if = "Option::is_none")]
        from_hook: Option<bool>,
    },
    #[serde(rename = "branch_summary")]
    BranchSummary {
        id: String,
        #[serde(rename = "parentId", default, skip_serializing_if = "Option::is_none")]
        parent_id: Option<String>,
        timestamp: String,
        #[serde(rename = "fromId")]
        from_id: String,
        summary: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        details: Option<Value>,
        #[serde(skip_serializing_if = "Option::is_none")]
        from_hook: Option<bool>,
    },
    #[serde(rename = "custom")]
    Custom {
        id: String,
        #[serde(rename = "parentId", default, skip_serializing_if = "Option::is_none")]
        parent_id: Option<String>,
        timestamp: String,
        #[serde(rename = "customType")]
        custom_type: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        data: Option<Value>,
    },
    #[serde(rename = "custom_message")]
    CustomMessage {
        id: String,
        #[serde(rename = "parentId", default, skip_serializing_if = "Option::is_none")]
        parent_id: Option<String>,
        timestamp: String,
        #[serde(rename = "customType")]
        custom_type: String,
        content: CustomMessageEntryContent,
        #[serde(skip_serializing_if = "Option::is_none")]
        details: Option<Value>,
        display: bool,
    },
    #[serde(rename = "label")]
    Label {
        id: String,
        #[serde(rename = "parentId", default, skip_serializing_if = "Option::is_none")]
        parent_id: Option<String>,
        timestamp: String,
        #[serde(rename = "targetId")]
        target_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        label: Option<String>,
    },
    #[serde(rename = "session_info")]
    SessionInfo {
        id: String,
        #[serde(rename = "parentId", default, skip_serializing_if = "Option::is_none")]
        parent_id: Option<String>,
        timestamp: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
    },
    #[serde(rename = "leaf")]
    Leaf {
        id: String,
        #[serde(rename = "parentId", default, skip_serializing_if = "Option::is_none")]
        parent_id: Option<String>,
        timestamp: String,
        #[serde(rename = "targetId")]
        target_id: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CustomMessageEntryContent {
    Text(String),
    Blocks(Vec<CustomMessageEntryBlock>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CustomMessageEntryBlock {
    Text(TextContent),
    Image(ImageContent),
}

impl SessionTreeEntry {
    pub fn id(&self) -> &str {
        match self {
            Self::Message { id, .. }
            | Self::ThinkingLevelChange { id, .. }
            | Self::ModelChange { id, .. }
            | Self::ActiveToolsChange { id, .. }
            | Self::Compaction { id, .. }
            | Self::BranchSummary { id, .. }
            | Self::Custom { id, .. }
            | Self::CustomMessage { id, .. }
            | Self::Label { id, .. }
            | Self::SessionInfo { id, .. }
            | Self::Leaf { id, .. } => id,
        }
    }

    pub fn parent_id(&self) -> Option<&str> {
        match self {
            Self::Message { parent_id, .. }
            | Self::ThinkingLevelChange { parent_id, .. }
            | Self::ModelChange { parent_id, .. }
            | Self::ActiveToolsChange { parent_id, .. }
            | Self::Compaction { parent_id, .. }
            | Self::BranchSummary { parent_id, .. }
            | Self::Custom { parent_id, .. }
            | Self::CustomMessage { parent_id, .. }
            | Self::Label { parent_id, .. }
            | Self::SessionInfo { parent_id, .. }
            | Self::Leaf { parent_id, .. } => parent_id.as_deref(),
        }
    }

    pub fn entry_type(&self) -> &'static str {
        match self {
            Self::Message { .. } => "message",
            Self::ThinkingLevelChange { .. } => "thinking_level_change",
            Self::ModelChange { .. } => "model_change",
            Self::ActiveToolsChange { .. } => "active_tools_change",
            Self::Compaction { .. } => "compaction",
            Self::BranchSummary { .. } => "branch_summary",
            Self::Custom { .. } => "custom",
            Self::CustomMessage { .. } => "custom_message",
            Self::Label { .. } => "label",
            Self::SessionInfo { .. } => "session_info",
            Self::Leaf { .. } => "leaf",
        }
    }
}

/// Session metadata with a stable identifier.
pub trait HasSessionId {
    fn session_id(&self) -> &str;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMetadata {
    pub id: String,
    #[serde(rename = "createdAt")]
    pub created_at: String,
}

impl HasSessionId for SessionMetadata {
    fn session_id(&self) -> &str {
        &self.id
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonlSessionMetadata {
    pub id: String,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    pub cwd: String,
    pub path: String,
    #[serde(rename = "parentSessionPath", skip_serializing_if = "Option::is_none")]
    pub parent_session_path: Option<String>,
}

impl HasSessionId for JsonlSessionMetadata {
    fn session_id(&self) -> &str {
        &self.id
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TursoSessionMetadata {
    pub id: String,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    pub db_path: String,
}

impl HasSessionId for TursoSessionMetadata {
    fn session_id(&self) -> &str {
        &self.id
    }
}

#[derive(Debug, Clone)]
pub struct SessionContext {
    pub messages: Vec<AgentMessage>,
    pub thinking_level: String,
    pub model: Option<SessionModelRef>,
    pub active_tool_names: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionModelRef {
    pub provider: String,
    pub model_id: String,
}

#[derive(Debug, Clone)]
pub struct SessionIndex {
    pub entries: Vec<SessionTreeEntry>,
    pub by_id: HashMap<String, SessionTreeEntry>,
    pub labels_by_id: HashMap<String, String>,
    pub leaf_id: Option<String>,
}

#[async_trait]
pub trait SessionStorage: Send + Sync {
    type Metadata: Clone + Send + Sync;

    async fn get_metadata(&self) -> Self::Metadata;
    async fn get_leaf_id(&self) -> Result<Option<String>, SessionError>;
    async fn set_leaf_id(&mut self, leaf_id: Option<String>) -> Result<(), SessionError>;
    async fn create_entry_id(&self) -> String;
    async fn append_entry(&mut self, entry: SessionTreeEntry) -> Result<(), SessionError>;
    async fn get_entry(&self, id: &str) -> Option<SessionTreeEntry>;
    async fn find_entries(&self, entry_type: &str) -> Vec<SessionTreeEntry>;
    async fn get_label(&self, id: &str) -> Option<String>;
    async fn get_path_to_root(&self, leaf_id: Option<&str>) -> Result<Vec<SessionTreeEntry>, SessionError>;
    async fn get_entries(&self) -> Vec<SessionTreeEntry>;
}
