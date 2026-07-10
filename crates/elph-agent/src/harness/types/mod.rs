//! Harness filesystem, execution, and agent-harness types — elph-agent module.

mod errors;
mod events;
mod filesystem;
mod hooks;
mod options;
mod result;

pub use errors::{
    AgentHarnessError, AgentHarnessErrorCode, BranchSummaryError, BranchSummaryErrorCode, CompactionError,
    CompactionErrorCode, ExecutionError, ExecutionErrorCode, FileError, FileErrorCode,
};
pub use events::{
    AbortEvent, AbortResult, AfterProviderResponseEvent, AgentHarnessOwnEvent, AgentHarnessPhase,
    BeforeAgentStartEvent, BeforeProviderPayloadEvent, BeforeProviderRequestEvent, BranchSummaryResult, CompactResult,
    CompactionPreparation, ContextEvent, FileOperations, ModelUpdateEvent, ModelUpdateSource, NavigateTreeResult,
    PendingSessionWrite, QueueUpdateEvent, ResourcesUpdateEvent, SavePointEvent, SessionBeforeCompactEvent,
    SessionBeforeTreeEvent, SessionCompactEvent, SessionTreeEvent, SettledEvent, ThinkingLevelUpdateEvent,
    ToolCallEvent, ToolResultEvent, ToolsUpdateEvent, TreePreparation, is_known_harness_hook_type,
};
pub use filesystem::{
    CreateDirOptions, CreateTempFileOptions, ExecutionEnv, FileInfo, FileKind, FileSystem, ReadTextLinesOptions,
    RemoveOptions, Shell, ShellExecOptions, ShellExecResult,
};
pub use hooks::{
    BeforeAgentStartResult, BeforeProviderPayloadResult, BeforeProviderRequestResult, BranchSummarySummary,
    ContextResult, HarnessHookResult, SessionBeforeCompactResult, SessionBeforeTreeResult, ToolCallHookResult,
    ToolResultPatch,
};
pub use options::{
    AgentHarnessOptions, AgentHarnessPromptOptions, AgentHarnessResources, AgentHarnessStreamOptions,
    AgentHarnessStreamOptionsPatch, CompactionSettings, DEFAULT_COMPACTION_SETTINGS, DEFAULT_SKILL_VALIDATION_SETTINGS,
    PromptTemplate, Skill, SkillLoadOptions, SkillValidationSettings, SystemPrompt, SystemPromptContext,
    SystemPromptFn, apply_stream_options_patch, clone_stream_options, resolve_project_skills_dirs,
    resolve_user_skills_dirs,
};
pub use result::{HarnessResult, Result, err, get_or_throw, get_or_undefined, ok, to_error};
