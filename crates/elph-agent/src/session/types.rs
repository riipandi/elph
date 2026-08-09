//! Session tree entry types and storage trait.

use std::collections::HashMap;
use std::future::Future;

use elph_ai::{ImageContent, TextContent};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::collaboration::CollaborationMode;
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
        /// Transcript prompt-card title (slash body without leading `/`).
        /// e.g. `skill:review fix this` or `my-template arg1`. Empty for free-form prompts.
        #[serde(default, rename = "promptTitle")]
        prompt_title: String,
        /// Prompt card kind: `"skill"`, `"template"`, or empty for free-form user messages.
        #[serde(default, rename = "promptKind")]
        prompt_kind: String,
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
    #[serde(rename = "collaboration_mode_change")]
    CollaborationModeChange {
        id: String,
        #[serde(rename = "parentId", default, skip_serializing_if = "Option::is_none")]
        parent_id: Option<String>,
        timestamp: String,
        mode: CollaborationMode,
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
            | Self::CollaborationModeChange { id, .. }
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
            | Self::CollaborationModeChange { parent_id, .. }
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
            Self::CollaborationModeChange { .. } => "collaboration_mode_change",
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

    pub fn timestamp(&self) -> &str {
        match self {
            Self::Message { timestamp, .. }
            | Self::ThinkingLevelChange { timestamp, .. }
            | Self::ModelChange { timestamp, .. }
            | Self::CollaborationModeChange { timestamp, .. }
            | Self::ActiveToolsChange { timestamp, .. }
            | Self::Compaction { timestamp, .. }
            | Self::BranchSummary { timestamp, .. }
            | Self::Custom { timestamp, .. }
            | Self::CustomMessage { timestamp, .. }
            | Self::Label { timestamp, .. }
            | Self::SessionInfo { timestamp, .. }
            | Self::Leaf { timestamp, .. } => timestamp,
        }
    }

    /// Denormalized role for `session_entries.role` (messages only).
    pub fn message_role(&self) -> Option<&str> {
        match self {
            Self::Message { message, .. } => Some(message.role()),
            _ => None,
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

/// Metadata for a multi-file session directory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionDirMetadata {
    pub id: String,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    /// Last activity time (updated when tree entries are appended).
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
    pub cwd: String,
    /// Absolute path to the session directory (`~/.local/share/elph/sessions/<SESSION_ID>/`).
    pub dir: String,
    #[serde(rename = "parentSessionId", skip_serializing_if = "Option::is_none")]
    pub parent_session_id: Option<String>,
}

impl HasSessionId for SessionDirMetadata {
    fn session_id(&self) -> &str {
        &self.id
    }
}

/// Metadata for a Turso/SQLite-backed session (Pi-aligned `sessions` row).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TursoSessionMetadata {
    pub id: String,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(rename = "updatedAt", default)]
    pub updated_at: String,
    /// Working directory for this session (`cwd` column; mirrors Pi sqlite-node).
    #[serde(default)]
    pub cwd: String,
    #[serde(rename = "parentSessionId", skip_serializing_if = "Option::is_none")]
    pub parent_session_id: Option<String>,
    #[serde(rename = "providerId", skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
    #[serde(rename = "modelId", skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
    #[serde(rename = "agentMode", skip_serializing_if = "Option::is_none")]
    pub agent_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Absolute path to the shared database file.
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
    pub collaboration_mode: CollaborationMode,
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
    pub checkpoints: HashMap<String, CheckpointTail>,
    pub name: Option<String>,
}

/// Statistics about a session's contents.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SessionStatistics {
    /// Total number of entries in the session.
    pub total_entries: u64,
    /// Number of message entries.
    pub message_count: u64,
    /// Number of compaction entries.
    pub compaction_count: u64,
    /// Number of branch summary entries.
    pub branch_summary_count: u64,
    /// Approximate token count if known, or 0.
    pub approximate_tokens: u64,
    /// Session name, if set.
    pub name: Option<String>,
}

/// A cursor position for reading entries in batches.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CursorPosition {
    /// The ID of the entry to start from (exclusive).
    pub after_id: String,
    /// Maximum number of entries to return.
    pub limit: u32,
}

/// A self-contained checkpoint representing a retained compaction tail.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointTail {
    /// The root entry ID of this checkpoint.
    pub root_id: String,
    /// Entries comprising the checkpoint tail.
    pub entries: Vec<SessionTreeEntry>,
    /// The leaf ID at the time of checkpoint creation.
    pub leaf_id: Option<String>,
    /// Timestamp when the checkpoint was created.
    pub created_at: String,
}

/// Retry lifecycle events for compaction and branch-summary operations.
#[derive(Debug, Clone)]
pub enum CompactionRetryEvent {
    /// Compaction attempt starting.
    Attempt { attempt: u32, max_retries: u32 },
    /// Compaction failed, will retry.
    Retry { attempt: u32, error: String, delay_ms: u64 },
    /// Compaction succeeded after retries.
    Recovered { attempt: u32 },
    /// Compaction permanently failed.
    Failed { error: String },
}

pub trait SessionStorage: Send + Sync {
    type Metadata: Clone + Send + Sync;

    fn get_metadata<'a>(&'a self) -> impl Future<Output = Self::Metadata> + Send + use<'a, Self>;
    fn get_leaf_id<'a>(&'a self) -> impl Future<Output = Result<Option<String>, SessionError>> + Send + use<'a, Self>;
    fn set_leaf_id<'a>(
        &'a mut self,
        leaf_id: Option<String>,
    ) -> impl Future<Output = Result<(), SessionError>> + Send + use<'a, Self>;
    fn create_entry_id<'a>(&'a self) -> impl Future<Output = String> + Send + use<'a, Self>;
    fn append_entry<'a>(
        &'a mut self,
        entry: SessionTreeEntry,
    ) -> impl Future<Output = Result<(), SessionError>> + Send + use<'a, Self>;
    fn get_entry<'a>(&'a self, id: &'a str) -> impl Future<Output = Option<SessionTreeEntry>> + Send + use<'a, Self>;
    fn find_entries<'a>(
        &'a self,
        entry_type: &'a str,
    ) -> impl Future<Output = Vec<SessionTreeEntry>> + Send + use<'a, Self>;
    fn get_label<'a>(&'a self, id: &'a str) -> impl Future<Output = Option<String>> + Send + use<'a, Self>;

    /// Get the path from the leaf to the root, or the nearest compaction.
    /// Returns entries from the compaction boundary (or root) up to the leaf.
    /// This is the v2 API replacing `get_path_to_root` — it stops at compaction
    /// boundaries to avoid loading the full session history.
    fn get_path_to_root_or_compaction<'a>(
        &'a self,
        leaf_id: Option<&'a str>,
    ) -> impl Future<Output = Result<Vec<SessionTreeEntry>, SessionError>> + Send + use<'a, Self>;

    /// Get all entries (legacy — prefer cursor-based reads for large sessions).
    fn get_entries<'a>(&'a self) -> impl Future<Output = Vec<SessionTreeEntry>> + Send + use<'a, Self>;

    /// Read entries in batches using a cursor.
    /// Returns entries after `after_id` (exclusive), up to `limit` entries.
    fn get_entries_cursor<'a>(
        &'a self,
        cursor: &'a CursorPosition,
    ) -> impl Future<Output = Result<Vec<SessionTreeEntry>, SessionError>> + Send + use<'a, Self>;

    /// Get session statistics (counts, name, approximate tokens).
    fn get_statistics<'a>(&'a self) -> impl Future<Output = SessionStatistics> + Send + use<'a, Self>;

    /// Store a compaction tail as a self-contained checkpoint.
    /// Returns the checkpoint ID on success.
    fn store_checkpoint_tail<'a>(
        &'a mut self,
        tail: CheckpointTail,
    ) -> impl Future<Output = Result<String, SessionError>> + Send + use<'a, Self>;

    /// Load a previously stored checkpoint tail by root ID.
    fn load_checkpoint_tail<'a>(
        &'a self,
        root_id: &'a str,
    ) -> impl Future<Output = Result<Option<CheckpointTail>, SessionError>> + Send + use<'a, Self>;

    /// List all stored checkpoint root IDs.
    fn list_checkpoint_tails<'a>(&'a self) -> impl Future<Output = Vec<String>> + Send + use<'a, Self>;

    /// Session name (optional, for display/labeling).
    fn get_name<'a>(&'a self) -> impl Future<Output = Option<String>> + Send + use<'a, Self> {
        async { None }
    }

    // ---------------------------------------------------------------------------
    // Default implementations for backward compatibility
    // ---------------------------------------------------------------------------

    /// Get path to root (default implementation delegates to `get_path_to_root_or_compaction`).
    fn get_path_to_root<'a>(
        &'a self,
        leaf_id: Option<&'a str>,
    ) -> impl Future<Output = Result<Vec<SessionTreeEntry>, SessionError>> + Send + use<'a, Self> {
        self.get_path_to_root_or_compaction(leaf_id)
    }

    /// Physically delete entries whose ids are **not** in `keep_ids`.
    ///
    /// Used after compaction to reclaim disk. Default is a no-op (0 deleted).
    /// Implementations must rebuild any in-memory index and keep leaf consistency.
    fn physical_prune_except<'a>(
        &'a mut self,
        _keep_ids: &'a [String],
    ) -> impl Future<Output = Result<usize, SessionError>> + Send + use<'a, Self> {
        async { Ok(0) }
    }
}
