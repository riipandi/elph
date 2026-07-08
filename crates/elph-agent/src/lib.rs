//! App-agnostic agent runtime for Elph applications.
//!
//! Rust port of [@earendil-works/pi-agent](https://github.com/earendil-works/pi/tree/main/packages/agent).
pub mod agent;
pub mod agent_loop;
pub mod builder;
pub mod compaction;
pub mod datastore;
pub mod env;
pub mod event_stream;
pub mod harness;
pub mod init;
pub mod mcp;
pub mod messages;
pub mod migration;
pub mod prompt_templates;
pub mod proxy;
pub mod runtime;
pub mod sandbox;
pub mod session;
pub mod skills;
pub mod subagent;
pub mod tools;
pub mod types;

pub use agent::{Agent, AgentListener, AgentOptions, AgentSubscription, PartialAgentState, default_model};
pub use agent_loop::{agent_loop, agent_loop_continue, run_agent_loop, run_agent_loop_continue};
pub use builder::{AgentBuilder, AgentInit};
pub use compaction::{
    BranchPreparation, BranchSummaryDetails, CollectEntriesResult, CompactionDetails, CompactionPreparation,
    CompactionResult, CompactionSettings, ContextUsageEstimate, CutPointResult, FileOperations,
    GenerateBranchSummaryOptions, SUMMARIZATION_SYSTEM_PROMPT, calculate_context_tokens,
    collect_entries_for_branch_summary, compact, compute_file_lists, create_file_ops, estimate_context_tokens,
    estimate_tokens, extract_file_ops_from_message, find_cut_point, find_turn_start_index, format_file_operations,
    generate_branch_summary, generate_summary, get_last_assistant_usage, prepare_branch_entries, prepare_compaction,
    serialize_conversation, should_compact,
};
pub use datastore::{DatabaseSpec, ensure_database, ensure_databases, ensure_databases_once};
pub use elph_ai::{OnPayloadCallback, OnResponseCallback};
pub use elph_core::logger::{LogRotation, LoggingOptions};
pub use elph_core::{ensure_dirs, write_file_if_missing, write_json_file, write_private_file};
pub use env::LocalExecutionEnv;
pub use event_stream::{AgentEventSink, AgentEventStream};
pub use harness::utils::TruncationResult;
pub use harness::{
    AfterProviderResponseEvent, AgentHarness, AgentHarnessError, AgentHarnessErrorCode, AgentHarnessEvent,
    AgentHarnessOptions, AgentHarnessOwnEvent, AgentHarnessPhase, AgentHarnessPromptOptions, AgentHarnessResources,
    AgentHarnessStreamOptions, AgentHarnessStreamOptionsPatch, BeforeAgentStartEvent, BeforeAgentStartResult,
    BeforeProviderPayloadEvent, BeforeProviderPayloadResult, BeforeProviderRequestEvent, BeforeProviderRequestResult,
    BranchSummaryError, BranchSummaryErrorCode, BranchSummaryResult, BranchSummarySummary, CompactResult,
    CompactionError, CompactionErrorCode, ContextEvent, ContextResult, CreateDirOptions, CreateTempFileOptions,
    DEFAULT_COMPACTION_SETTINGS as HARNESS_DEFAULT_COMPACTION_SETTINGS, DEFAULT_MAX_BYTES, DEFAULT_MAX_LINES,
    ExecutionEnv, ExecutionError, ExecutionErrorCode, FileError, FileErrorCode, FileInfo, FileKind,
    FileOperations as HarnessFileOperations, FileSystem, GREP_MAX_LINE_LENGTH, HarnessHookResult, HarnessOpResult,
    HarnessResult, ModelUpdateEvent, ModelUpdateSource, NavigateTreeOptions, NavigateTreeResult, PendingSessionWrite,
    PromptTemplate, QueueUpdateEvent, ReadTextLinesOptions, RemoveOptions, ResourcesUpdateEvent,
    Result as HarnessTypedResult, SavePointEvent, SessionBeforeCompactEvent, SessionBeforeCompactResult,
    SessionBeforeTreeEvent, SessionBeforeTreeResult, SessionCompactEvent, SessionTreeEvent, SettledEvent, Shell,
    ShellCaptureOptions, ShellExecOptions, ShellExecResult, Skill, SystemPrompt, SystemPromptContext, SystemPromptFn,
    ThinkingLevelUpdateEvent, ToolCallEvent, ToolCallHookResult, ToolResultEvent, ToolResultPatch, ToolsUpdateEvent,
    TreePreparation, TruncatedBy, TruncationOptions, err, execute_shell_with_capture, finalize_shell_capture,
    format_size, format_skills_for_system_prompt, get_or_throw, get_or_undefined, is_known_harness_hook_type, ok,
    sanitize_binary_output, to_error, truncate_head, truncate_line, truncate_tail,
};
pub use init::InitProgress;
pub use messages::{
    CustomMessageContent, bash_execution_to_text, create_branch_summary_message, create_compaction_summary_message,
    create_custom_message, default_convert_to_llm, default_convert_to_llm as convert_to_llm, default_convert_to_llm_fn,
    now_iso_timestamp,
};
pub use migration::Migration;
pub use prompt_templates::{
    LoadPromptTemplatesResult, LoadSourcedPromptTemplatesResult, PromptTemplateDiagnostic,
    PromptTemplateDiagnosticCode, SourcedPromptTemplate, SourcedPromptTemplateDiagnostic,
    format_prompt_template_invocation, load_prompt_templates, load_sourced_prompt_templates, parse_command_args,
    substitute_args,
};
pub use proxy::{ProxyAssistantMessageEvent, ProxyStreamOptions, stream_proxy};
pub use runtime::{block_on, try_block_on};
pub use session::id::uuidv7;
pub use session::{
    BranchSummaryOptions, CustomMessageEntryBlock, CustomMessageEntryContent, ForkEntriesOptions, ForkPosition,
    InMemorySessionCreateOptions, InMemorySessionOptions, InMemorySessionRepo, InMemorySessionStorage,
    JsonlSessionCreateOptions, JsonlSessionListOptions, JsonlSessionMetadata, JsonlSessionRepo,
    JsonlSessionRepoCreateOptions, JsonlSessionStorage, SESSION_TREE_MIGRATIONS, Session, SessionContext, SessionError,
    SessionErrorCode, SessionMetadata, SessionModelRef, SessionStorage, SessionTreeEntry, TursoSessionMetadata,
    TursoSessionStorage, build_session_context, create_session_id, create_timestamp, get_entries_to_fork,
    load_jsonl_session_metadata, to_session,
};
pub use skills::{
    LoadSkillsResult, LoadSourcedSkillsResult, SkillDiagnostic, SkillDiagnosticCode, SourcedSkill,
    SourcedSkillDiagnostic, format_skill_invocation, load_skills, load_skills_with_options, load_sourced_skills,
    load_sourced_skills_with_options,
};
pub use tools::{WebSearchEngine, WebSearchResult};
pub use tools::{
    create_all_tools, create_all_tools_with_web, create_bash_tool, create_coding_tools, create_edit_tool,
    create_find_tool, create_grep_tool, create_ls_tool, create_read_only_tools, create_read_tool,
    create_web_fetch_tool, create_web_search_tool, create_web_tools, create_write_tool, echo_tool, simple_tool,
};
pub use types::*;
