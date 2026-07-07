//! Agent harness — elph-agent module.

pub mod agent_harness;
pub mod generic_on;
pub mod hooks;
pub mod system_prompt;
pub mod types;
pub mod utils;

pub use agent_harness::{AgentHarness, HarnessOpResult, NavigateTreeOptions};
pub use hooks::{AgentHarnessEvent, HookRegistry, SUBSCRIBER_EVENT_TYPE};
pub use system_prompt::format_skills_for_system_prompt;
pub use types::{
    AbortEvent, AbortResult, AfterProviderResponseEvent, AgentHarnessError, AgentHarnessErrorCode, AgentHarnessOptions,
    AgentHarnessOwnEvent, AgentHarnessPhase, AgentHarnessPromptOptions, AgentHarnessResources,
    AgentHarnessStreamOptions, AgentHarnessStreamOptionsPatch, BeforeAgentStartEvent, BeforeAgentStartResult,
    BeforeProviderPayloadEvent, BeforeProviderPayloadResult, BeforeProviderRequestEvent, BeforeProviderRequestResult,
    BranchSummaryError, BranchSummaryErrorCode, BranchSummaryResult, BranchSummarySummary, CompactResult,
    CompactionError, CompactionErrorCode, CompactionPreparation, CompactionSettings, ContextEvent, ContextResult,
    CreateDirOptions, CreateTempFileOptions, DEFAULT_COMPACTION_SETTINGS, ExecutionEnv, ExecutionError,
    ExecutionErrorCode, FileError, FileErrorCode, FileInfo, FileKind, FileOperations, FileSystem, HarnessHookResult,
    HarnessResult, ModelUpdateEvent, ModelUpdateSource, NavigateTreeResult, PendingSessionWrite, PromptTemplate,
    QueueUpdateEvent, ReadTextLinesOptions, RemoveOptions, ResourcesUpdateEvent, Result, SavePointEvent,
    SessionBeforeCompactEvent, SessionBeforeCompactResult, SessionBeforeTreeEvent, SessionBeforeTreeResult,
    SessionCompactEvent, SessionTreeEvent, SettledEvent, Shell, ShellExecOptions, ShellExecResult, Skill, SystemPrompt,
    SystemPromptContext, SystemPromptFn, ThinkingLevelUpdateEvent, ToolCallEvent, ToolCallHookResult, ToolResultEvent,
    ToolResultPatch, ToolsUpdateEvent, TreePreparation, err, get_or_throw, get_or_undefined,
    is_known_harness_hook_type, ok, to_error,
};
pub use utils::{
    DEFAULT_MAX_BYTES, DEFAULT_MAX_LINES, GREP_MAX_LINE_LENGTH, ShellCaptureOptions, TruncatedBy, TruncationOptions,
    execute_shell_with_capture, finalize_shell_capture, format_size, sanitize_binary_output, truncate_head,
    truncate_line, truncate_tail,
};
