//! Harness session, compaction, and event types.

use std::collections::HashSet;

use elph_ai::{ImageContent, Model};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::session::SessionTreeEntry;
use crate::types::{AgentMessage, AgentThinkingLevel, ToolResultContent};

use super::options::{AgentHarnessResources, AgentHarnessStreamOptions, CompactionSettings};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentHarnessPhase {
    Idle,
    Turn,
    Compaction,
    BranchSummary,
    Retry,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum PendingSessionWrite {
    #[serde(rename = "message")]
    Message { message: AgentMessage },
    #[serde(rename = "thinking_level_change")]
    ThinkingLevelChange {
        #[serde(rename = "thinkingLevel")]
        thinking_level: String,
    },
    #[serde(rename = "model_change")]
    ModelChange {
        provider: String,
        #[serde(rename = "modelId")]
        model_id: String,
    },
    #[serde(rename = "active_tools_change")]
    ActiveToolsChange {
        #[serde(rename = "activeToolNames")]
        active_tool_names: Vec<String>,
    },
    #[serde(rename = "compaction")]
    Compaction {
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
        #[serde(rename = "customType")]
        custom_type: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        data: Option<Value>,
    },
    #[serde(rename = "custom_message")]
    CustomMessage {
        #[serde(rename = "customType")]
        custom_type: String,
        content: crate::session::CustomMessageEntryContent,
        #[serde(skip_serializing_if = "Option::is_none")]
        details: Option<Value>,
        display: bool,
    },
    #[serde(rename = "label")]
    Label {
        #[serde(rename = "targetId")]
        target_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        label: Option<String>,
    },
    #[serde(rename = "session_info")]
    SessionInfo {
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
    },
    #[serde(rename = "leaf")]
    Leaf {
        #[serde(rename = "targetId")]
        target_id: Option<String>,
    },
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileOperations {
    pub read: HashSet<String>,
    pub written: HashSet<String>,
    pub edited: HashSet<String>,
}

#[derive(Debug, Clone)]
pub struct CompactionPreparation {
    pub first_kept_entry_id: String,
    pub messages_to_summarize: Vec<AgentMessage>,
    pub turn_prefix_messages: Vec<AgentMessage>,
    pub is_split_turn: bool,
    pub tokens_before: u64,
    pub previous_summary: Option<String>,
    pub file_ops: FileOperations,
    pub settings: CompactionSettings,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompactResult {
    pub summary: String,
    pub first_kept_entry_id: String,
    pub tokens_before: u64,
    pub details: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchSummaryResult {
    pub summary: String,
    pub read_files: Vec<String>,
    pub modified_files: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct TreePreparation {
    pub target_id: String,
    pub old_leaf_id: Option<String>,
    pub common_ancestor_id: Option<String>,
    pub entries_to_summarize: Vec<SessionTreeEntry>,
    pub user_wants_summary: bool,
    pub custom_instructions: Option<String>,
    pub replace_instructions: bool,
    pub label: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AbortResult {
    pub cleared_steer: Vec<AgentMessage>,
    pub cleared_follow_up: Vec<AgentMessage>,
}

#[derive(Debug, Clone)]
pub struct NavigateTreeResult {
    pub cancelled: bool,
    pub editor_text: Option<String>,
    pub summary_entry: Option<SessionTreeEntry>,
}

// ---------------------------------------------------------------------------
// Harness event types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct QueueUpdateEvent {
    pub steer: Vec<AgentMessage>,
    pub follow_up: Vec<AgentMessage>,
    pub next_turn: Vec<AgentMessage>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SavePointEvent {
    pub had_pending_mutations: bool,
}

#[derive(Debug, Clone)]
pub struct AbortEvent {
    pub cleared_steer: Vec<AgentMessage>,
    pub cleared_follow_up: Vec<AgentMessage>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SettledEvent {
    pub next_turn_count: usize,
}

#[derive(Debug, Clone)]
pub struct BeforeAgentStartEvent {
    pub prompt: String,
    pub images: Option<Vec<ImageContent>>,
    pub system_prompt: String,
    pub resources: AgentHarnessResources,
}

#[derive(Debug, Clone)]
pub struct ContextEvent {
    pub messages: Vec<AgentMessage>,
}

#[derive(Debug, Clone)]
pub struct BeforeProviderRequestEvent {
    pub model: Model,
    pub session_id: String,
    pub stream_options: AgentHarnessStreamOptions,
}

#[derive(Debug, Clone)]
pub struct BeforeProviderPayloadEvent {
    pub model: Model,
    pub payload: Value,
}

#[derive(Debug, Clone)]
pub struct AfterProviderResponseEvent {
    pub status: u16,
    pub headers: std::collections::HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct ToolCallEvent {
    pub tool_call_id: String,
    pub tool_name: String,
    pub input: Value,
}

#[derive(Debug, Clone)]
pub struct ToolResultEvent {
    pub tool_call_id: String,
    pub tool_name: String,
    pub input: Value,
    pub content: Vec<ToolResultContent>,
    pub details: Value,
    pub is_error: bool,
}

#[derive(Debug, Clone)]
pub struct SessionBeforeCompactEvent {
    pub preparation: CompactionPreparation,
    pub branch_entries: Vec<SessionTreeEntry>,
    pub custom_instructions: Option<String>,
    pub abort_token: CancellationToken,
}

#[derive(Debug, Clone)]
pub struct SessionCompactEvent {
    pub compaction_entry: SessionTreeEntry,
    pub from_hook: bool,
}

#[derive(Debug, Clone)]
pub struct SessionBeforeTreeEvent {
    pub preparation: TreePreparation,
    pub abort_token: CancellationToken,
}

#[derive(Debug, Clone)]
pub struct SessionTreeEvent {
    pub new_leaf_id: Option<String>,
    pub old_leaf_id: Option<String>,
    pub summary_entry: Option<SessionTreeEntry>,
    pub from_hook: Option<bool>,
}

#[derive(Debug, Clone)]
pub struct ModelUpdateEvent {
    pub model: Model,
    pub previous_model: Option<Model>,
    pub source: ModelUpdateSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelUpdateSource {
    Set,
    Restore,
}

#[derive(Debug, Clone)]
pub struct ThinkingLevelUpdateEvent {
    pub level: AgentThinkingLevel,
    pub previous_level: AgentThinkingLevel,
}

#[derive(Debug, Clone)]
pub struct ToolsUpdateEvent {
    pub tool_names: Vec<String>,
    pub previous_tool_names: Vec<String>,
    pub active_tool_names: Vec<String>,
    pub previous_active_tool_names: Vec<String>,
    pub source: ModelUpdateSource,
}

#[derive(Debug, Clone)]
pub struct ResourcesUpdateEvent {
    pub resources: AgentHarnessResources,
    pub previous_resources: AgentHarnessResources,
}

#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum AgentHarnessOwnEvent {
    QueueUpdate(QueueUpdateEvent),
    SavePoint(SavePointEvent),
    Abort(AbortEvent),
    Settled(SettledEvent),
    BeforeAgentStart(BeforeAgentStartEvent),
    Context(ContextEvent),
    BeforeProviderRequest(BeforeProviderRequestEvent),
    BeforeProviderPayload(BeforeProviderPayloadEvent),
    AfterProviderResponse(AfterProviderResponseEvent),
    ToolCall(ToolCallEvent),
    ToolResult(ToolResultEvent),
    SessionBeforeCompact(SessionBeforeCompactEvent),
    SessionCompact(SessionCompactEvent),
    SessionBeforeTree(SessionBeforeTreeEvent),
    SessionTree(SessionTreeEvent),
    ModelUpdate(ModelUpdateEvent),
    ThinkingLevelUpdate(ThinkingLevelUpdateEvent),
    ToolsUpdate(ToolsUpdateEvent),
    ResourcesUpdate(ResourcesUpdateEvent),
}

impl AgentHarnessOwnEvent {
    /// Snake-case hook name matching upstream `AgentHarnessEventResultMap` keys.
    pub fn hook_type(&self) -> &'static str {
        match self {
            Self::QueueUpdate(_) => "queue_update",
            Self::SavePoint(_) => "save_point",
            Self::Abort(_) => "abort",
            Self::Settled(_) => "settled",
            Self::BeforeAgentStart(_) => "before_agent_start",
            Self::Context(_) => "context",
            Self::BeforeProviderRequest(_) => "before_provider_request",
            Self::BeforeProviderPayload(_) => "before_provider_payload",
            Self::AfterProviderResponse(_) => "after_provider_response",
            Self::ToolCall(_) => "tool_call",
            Self::ToolResult(_) => "tool_result",
            Self::SessionBeforeCompact(_) => "session_before_compact",
            Self::SessionCompact(_) => "session_compact",
            Self::SessionBeforeTree(_) => "session_before_tree",
            Self::SessionTree(_) => "session_tree",
            Self::ModelUpdate(_) => "model_update",
            Self::ThinkingLevelUpdate(_) => "thinking_level_update",
            Self::ToolsUpdate(_) => "tools_update",
            Self::ResourcesUpdate(_) => "resources_update",
        }
    }
}

/// Returns `true` when `event_type` is a known upstream harness hook name.
pub fn is_known_harness_hook_type(event_type: &str) -> bool {
    matches!(
        event_type,
        "before_agent_start"
            | "context"
            | "before_provider_request"
            | "before_provider_payload"
            | "after_provider_response"
            | "tool_call"
            | "tool_result"
            | "session_before_compact"
            | "session_compact"
            | "session_before_tree"
            | "session_tree"
            | "model_update"
            | "thinking_level_update"
            | "tools_update"
            | "resources_update"
            | "queue_update"
            | "save_point"
            | "abort"
            | "settled"
    )
}
