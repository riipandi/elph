//! Harness filesystem, execution, and agent-harness types — elph-agent module.

use std::collections::HashSet;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;
use elph_ai::{ImageContent, Model, Models, Transport};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::session::{Session, SessionTreeEntry};
use crate::types::{AgentMessage, AgentThinkingLevel, AgentTool, QueueMode, ToolResultContent};

/// Fallible harness operation result. Expected failures are returned as `Err` instead of thrown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Result<T, E> {
    Ok(T),
    Err(E),
}

impl<T, E> Result<T, E> {
    pub fn is_ok(&self) -> bool {
        matches!(self, Self::Ok(_))
    }

    pub fn is_err(&self) -> bool {
        matches!(self, Self::Err(_))
    }

    pub fn unwrap(self) -> T
    where
        E: std::fmt::Debug,
    {
        match self {
            Self::Ok(value) => value,
            Self::Err(error) => panic!("called `Result::unwrap()` on an `Err` value: {error:?}"),
        }
    }

    pub fn expect(self, message: &str) -> T
    where
        E: std::fmt::Debug,
    {
        match self {
            Self::Ok(value) => value,
            Self::Err(error) => panic!("{message}: {error:?}"),
        }
    }
}

/// Standard `Result` alias used by compaction and summarization helpers.
pub type HarnessResult<T, E> = std::result::Result<T, E>;

pub fn ok<T, E>(value: T) -> Result<T, E> {
    Result::Ok(value)
}

pub fn err<T, E>(error: E) -> Result<T, E> {
    Result::Err(error)
}

/// Return the success value or panic with the failure error.
pub fn get_or_throw<T, E: std::fmt::Display>(result: Result<T, E>) -> T {
    match result {
        Result::Ok(value) => value,
        Result::Err(error) => panic!("{error}"),
    }
}

/// Return the success value or `None`.
pub fn get_or_undefined<T, E>(result: Result<T, E>) -> Option<T> {
    match result {
        Result::Ok(value) => Some(value),
        Result::Err(_) => None,
    }
}

/// Normalize unknown thrown values into displayable error messages.
pub fn to_error(error: impl std::fmt::Display) -> String {
    error.to_string()
}

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileErrorCode {
    Aborted,
    NotFound,
    PermissionDenied,
    NotDirectory,
    IsDirectory,
    Invalid,
    NotSupported,
    Unknown,
}

#[derive(Debug, Clone)]
pub struct FileError {
    pub code: FileErrorCode,
    pub message: String,
    pub path: Option<String>,
}

impl FileError {
    pub fn new(code: FileErrorCode, message: impl Into<String>, path: Option<String>) -> Self {
        Self {
            code,
            message: message.into(),
            path,
        }
    }
}

impl std::fmt::Display for FileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for FileError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionErrorCode {
    Aborted,
    Timeout,
    ShellUnavailable,
    SpawnError,
    CallbackError,
    Unknown,
}

#[derive(Debug, Clone)]
pub struct ExecutionError {
    pub code: ExecutionErrorCode,
    pub message: String,
}

impl ExecutionError {
    pub fn new(code: ExecutionErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for ExecutionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ExecutionError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompactionErrorCode {
    Aborted,
    SummarizationFailed,
    InvalidSession,
    Unknown,
}

#[derive(Debug, Clone)]
pub struct CompactionError {
    pub code: CompactionErrorCode,
    pub message: String,
}

impl CompactionError {
    pub fn new(code: CompactionErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for CompactionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for CompactionError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BranchSummaryErrorCode {
    Aborted,
    SummarizationFailed,
    InvalidSession,
}

#[derive(Debug, Clone)]
pub struct BranchSummaryError {
    pub code: BranchSummaryErrorCode,
    pub message: String,
}

impl BranchSummaryError {
    pub fn new(code: BranchSummaryErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for BranchSummaryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for BranchSummaryError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentHarnessErrorCode {
    Busy,
    InvalidState,
    InvalidArgument,
    Session,
    Hook,
    Auth,
    Compaction,
    BranchSummary,
    Unknown,
}

#[derive(Debug, Clone)]
pub struct AgentHarnessError {
    pub code: AgentHarnessErrorCode,
    pub message: String,
}

impl AgentHarnessError {
    pub fn new(code: AgentHarnessErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for AgentHarnessError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for AgentHarnessError {}

// ---------------------------------------------------------------------------
// Filesystem types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FileKind {
    File,
    Directory,
    Symlink,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileInfo {
    pub name: String,
    pub path: String,
    pub kind: FileKind,
    pub size: u64,
    pub mtime_ms: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub content: String,
    pub file_path: String,
    pub disable_model_invocation: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptTemplate {
    pub name: String,
    pub description: String,
    pub content: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AgentHarnessResources {
    pub prompt_templates: Vec<PromptTemplate>,
    pub skills: Vec<Skill>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AgentHarnessStreamOptions {
    pub transport: Option<Transport>,
    pub timeout_ms: Option<u64>,
    pub max_retries: Option<u32>,
    pub max_retry_delay_ms: Option<u64>,
    pub headers: Option<std::collections::HashMap<String, String>>,
    pub metadata: Option<Value>,
    pub cache_retention: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AgentHarnessStreamOptionsPatch {
    pub transport: Option<Transport>,
    /// `None` = leave unchanged, `Some(None)` = clear, `Some(Some(v))` = set.
    pub timeout_ms: Option<Option<u64>>,
    /// `None` = leave unchanged, `Some(None)` = clear, `Some(Some(v))` = set.
    pub max_retries: Option<Option<u32>>,
    /// `None` = leave unchanged, `Some(None)` = clear, `Some(Some(v))` = set.
    pub max_retry_delay_ms: Option<Option<u64>>,
    /// `None` = leave unchanged, `Some(None)` = clear all headers, `Some(Some(map))` = merge/delete keys.
    pub headers: Option<Option<std::collections::HashMap<String, Option<String>>>>,
    /// `None` = leave unchanged, `Some(None)` = clear all metadata, `Some(Some(map))` = merge/delete keys.
    pub metadata: Option<Option<std::collections::HashMap<String, Option<Value>>>>,
    pub cache_retention: Option<String>,
}

pub fn clone_stream_options(stream_options: &AgentHarnessStreamOptions) -> AgentHarnessStreamOptions {
    AgentHarnessStreamOptions {
        transport: stream_options.transport,
        timeout_ms: stream_options.timeout_ms,
        max_retries: stream_options.max_retries,
        max_retry_delay_ms: stream_options.max_retry_delay_ms,
        headers: stream_options.headers.clone(),
        metadata: stream_options.metadata.clone(),
        cache_retention: stream_options.cache_retention.clone(),
    }
}

pub fn apply_stream_options_patch(
    base: AgentHarnessStreamOptions,
    patch: &AgentHarnessStreamOptionsPatch,
) -> AgentHarnessStreamOptions {
    let mut result = clone_stream_options(&base);
    if patch.transport.is_some() {
        result.transport = patch.transport;
    }
    if let Some(timeout_ms) = patch.timeout_ms {
        result.timeout_ms = timeout_ms;
    }
    if let Some(max_retries) = patch.max_retries {
        result.max_retries = max_retries;
    }
    if let Some(max_retry_delay_ms) = patch.max_retry_delay_ms {
        result.max_retry_delay_ms = max_retry_delay_ms;
    }
    if patch.cache_retention.is_some() {
        result.cache_retention = patch.cache_retention.clone();
    }
    if let Some(headers_patch) = &patch.headers {
        result.headers = match headers_patch {
            None => None,
            Some(map) => {
                let mut headers = result.headers.take().unwrap_or_default();
                for (key, value) in map {
                    match value {
                        Some(value) => {
                            headers.insert(key.clone(), value.clone());
                        }
                        None => {
                            headers.remove(key);
                        }
                    }
                }
                if headers.is_empty() { None } else { Some(headers) }
            }
        };
    }
    if let Some(metadata_patch) = &patch.metadata {
        result.metadata = match metadata_patch {
            None => None,
            Some(map) => {
                let mut metadata = result
                    .metadata
                    .as_ref()
                    .and_then(|value| value.as_object())
                    .cloned()
                    .unwrap_or_default();
                for (key, value) in map {
                    match value {
                        Some(value) => {
                            metadata.insert(key.clone(), value.clone());
                        }
                        None => {
                            metadata.remove(key);
                        }
                    }
                }
                if metadata.is_empty() {
                    None
                } else {
                    Some(serde_json::Value::Object(metadata))
                }
            }
        };
    }
    result
}

#[derive(Debug, Clone)]
pub struct ReadTextLinesOptions {
    pub max_lines: Option<usize>,
    pub abort_token: Option<CancellationToken>,
}

#[derive(Debug, Clone)]
pub struct CreateDirOptions {
    pub recursive: bool,
    pub abort_token: Option<CancellationToken>,
}

impl Default for CreateDirOptions {
    fn default() -> Self {
        Self {
            recursive: true,
            abort_token: None,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct RemoveOptions {
    pub recursive: bool,
    pub force: bool,
    pub abort_token: Option<CancellationToken>,
}

#[derive(Debug, Clone, Default)]
pub struct CreateTempFileOptions {
    pub prefix: String,
    pub suffix: String,
    pub abort_token: Option<CancellationToken>,
}

/// Filesystem capability used by the harness.
#[async_trait]
pub trait FileSystem: Send + Sync {
    fn cwd(&self) -> &str;

    async fn absolute_path(&self, path: &str, abort_token: Option<&CancellationToken>) -> Result<String, FileError>;
    async fn join_path(&self, parts: &[&str], abort_token: Option<&CancellationToken>) -> Result<String, FileError>;
    async fn read_text_file(&self, path: &str, abort_token: Option<&CancellationToken>) -> Result<String, FileError>;
    async fn read_text_lines(
        &self,
        path: &str,
        options: Option<ReadTextLinesOptions>,
    ) -> Result<Vec<String>, FileError>;
    async fn read_binary_file(&self, path: &str, abort_token: Option<&CancellationToken>)
    -> Result<Vec<u8>, FileError>;
    async fn write_file(
        &self,
        path: &str,
        content: &[u8],
        abort_token: Option<&CancellationToken>,
    ) -> Result<(), FileError>;
    async fn append_file(
        &self,
        path: &str,
        content: &[u8],
        abort_token: Option<&CancellationToken>,
    ) -> Result<(), FileError>;
    async fn file_info(&self, path: &str, abort_token: Option<&CancellationToken>) -> Result<FileInfo, FileError>;
    async fn list_dir(&self, path: &str, abort_token: Option<&CancellationToken>) -> Result<Vec<FileInfo>, FileError>;
    async fn canonical_path(&self, path: &str, abort_token: Option<&CancellationToken>) -> Result<String, FileError>;
    async fn exists(&self, path: &str, abort_token: Option<&CancellationToken>) -> Result<bool, FileError>;
    async fn create_dir(&self, path: &str, options: Option<CreateDirOptions>) -> Result<(), FileError>;
    async fn remove(&self, path: &str, options: Option<RemoveOptions>) -> Result<(), FileError>;
    async fn create_temp_dir(&self, prefix: &str, abort_token: Option<&CancellationToken>)
    -> Result<String, FileError>;
    async fn create_temp_file(&self, options: Option<CreateTempFileOptions>) -> Result<String, FileError>;
    async fn cleanup(&self);
}

#[allow(clippy::type_complexity)]
#[derive(Clone)]
pub struct ShellExecOptions {
    pub cwd: Option<String>,
    pub env: Option<std::collections::HashMap<String, String>>,
    pub timeout: Option<u64>,
    pub abort_token: Option<CancellationToken>,
    pub on_stdout: Option<Arc<dyn Fn(&str) + Send + Sync>>,
    pub on_stderr: Option<Arc<dyn Fn(&str) + Send + Sync>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellExecResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

/// Shell execution capability used by the harness.
#[async_trait]
pub trait Shell: Send + Sync {
    async fn exec(&self, command: &str, options: Option<ShellExecOptions>) -> Result<ShellExecResult, ExecutionError>;
    async fn cleanup(&self);
}

/// Filesystem and process execution environment used by the harness.
#[async_trait]
pub trait ExecutionEnv: FileSystem + Shell {}

// ---------------------------------------------------------------------------
// Session write / phase types
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Compaction / branch summary types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileOperations {
    pub read: HashSet<String>,
    pub written: HashSet<String>,
    pub edited: HashSet<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompactionSettings {
    pub enabled: bool,
    pub reserve_tokens: u64,
    pub keep_recent_tokens: u64,
}

pub const DEFAULT_COMPACTION_SETTINGS: CompactionSettings = CompactionSettings {
    enabled: true,
    reserve_tokens: 16384,
    keep_recent_tokens: 20000,
};

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

// ---------------------------------------------------------------------------
// Harness hook result types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct BeforeAgentStartResult {
    pub messages: Option<Vec<AgentMessage>>,
    pub system_prompt: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ContextResult {
    pub messages: Vec<AgentMessage>,
}

#[derive(Debug, Clone, Default)]
pub struct BeforeProviderRequestResult {
    pub stream_options: Option<AgentHarnessStreamOptionsPatch>,
}

#[derive(Debug, Clone)]
pub struct BeforeProviderPayloadResult {
    pub payload: Value,
}

#[derive(Debug, Clone, Default)]
pub struct ToolCallHookResult {
    pub block: bool,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ToolResultPatch {
    pub content: Option<Vec<ToolResultContent>>,
    pub details: Option<Value>,
    pub is_error: Option<bool>,
    pub terminate: Option<bool>,
}

#[derive(Debug, Clone, Default)]
pub struct SessionBeforeCompactResult {
    pub cancel: bool,
    pub compaction: Option<CompactResult>,
    pub custom_instructions: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct SessionBeforeTreeResult {
    pub cancel: bool,
    pub summary: Option<BranchSummarySummary>,
    pub custom_instructions: Option<String>,
    pub replace_instructions: bool,
    pub label: Option<String>,
}

#[derive(Debug, Clone)]
pub struct BranchSummarySummary {
    pub summary: String,
    pub details: Option<Value>,
}

/// Result returned from generic [`AgentHarness::on`](super::agent_harness::AgentHarness::on) handlers.
#[derive(Debug, Clone)]
pub enum HarnessHookResult {
    BeforeAgentStart(BeforeAgentStartResult),
    Context(ContextResult),
    BeforeProviderRequest(BeforeProviderRequestResult),
    BeforeProviderPayload(BeforeProviderPayloadResult),
    ToolCall(ToolCallHookResult),
    ToolResult(ToolResultPatch),
    SessionBeforeCompact(SessionBeforeCompactResult),
    SessionBeforeTree(SessionBeforeTreeResult),
}

#[derive(Debug, Clone, Default)]
pub struct AgentHarnessPromptOptions {
    pub images: Option<Vec<ImageContent>>,
}

// ---------------------------------------------------------------------------
// Agent harness options
// ---------------------------------------------------------------------------

pub struct SystemPromptContext<S: crate::session::types::SessionStorage> {
    pub env: Arc<dyn ExecutionEnv>,
    pub session: Session<S>,
    pub model: Model,
    pub thinking_level: AgentThinkingLevel,
    pub active_tools: Vec<AgentTool>,
    pub resources: AgentHarnessResources,
}

pub type SystemPromptFn<S> =
    Arc<dyn Fn(SystemPromptContext<S>) -> Pin<Box<dyn Future<Output = String> + Send>> + Send + Sync>;

pub enum SystemPrompt<S: crate::session::types::SessionStorage> {
    Static(String),
    Dynamic(SystemPromptFn<S>),
}

pub struct AgentHarnessOptions<S>
where
    S: crate::session::types::SessionStorage + Send + Sync + 'static,
    S::Metadata: crate::session::types::HasSessionId + Send + Sync,
{
    pub env: Arc<dyn ExecutionEnv>,
    pub session: Session<S>,
    pub models: Arc<Models>,
    pub tools: Vec<AgentTool>,
    pub resources: AgentHarnessResources,
    pub system_prompt: SystemPrompt<S>,
    pub stream_options: AgentHarnessStreamOptions,
    pub model: Model,
    pub thinking_level: AgentThinkingLevel,
    pub active_tool_names: Vec<String>,
    pub steering_mode: QueueMode,
    pub follow_up_mode: QueueMode,
}
