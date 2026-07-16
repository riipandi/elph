//! App-agnostic agent runtime for Elph applications.
//!
//! Rust port of [@earendil-works/pi-agent](https://github.com/earendil-works/pi/tree/main/packages/agent).
pub mod agent;
pub mod builder;
pub mod compaction;
pub mod datastore;

pub mod goals;

pub mod messages;

pub mod collaboration;
#[cfg(feature = "extensions")]
pub mod plugins;
pub mod prompt;

pub mod runtime;

pub mod session;
pub mod skills;

pub mod tools;
pub mod trace;
pub mod types;

pub use agent::harness::utils::TruncationResult;
pub use agent::harness::{
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
pub use agent::subagent::{
    AgentControl, AgentGraphStore, AgentRegistry, SubagentBootstrap, SubagentEventForwarder, SubagentHarness,
    SubagentInfo, SubagentLimits, SubagentSpawnConfig, SubagentStatus, generate_agent_name,
};
pub use agent::{Agent, AgentListener, AgentOptions, AgentSubscription, PartialAgentState, default_model};
pub use builder::InitProgress;
pub use builder::{AgentBuilder, AgentInit, BuiltinToolsBuilder};
pub use collaboration::{
    CollaborationMode, EXPLORATION_BUILTIN_TOOLS, PlanConfirmationChoice, assistant_message_text,
    extract_proposed_plan, filter_active_tools, filter_ask_mode_tools, implement_prompt, is_ask_mode_tool,
    is_collaboration_tool, is_exploration_builtin_tool, is_mcp_read_only_bridge_tool, is_mcp_tool, is_mutating_tool,
    is_read_only_mcp_tool, plan_mode_block_reason, plan_mode_blocks_tool, plan_mode_system_prompt,
};
pub use compaction::{
    BranchPreparation, BranchSummaryDetails, CollectEntriesResult, CompactionDetails, CompactionPreparation,
    CompactionResult, CompactionSettings, ContextUsageEstimate, CutPointResult, FileOperations,
    GenerateBranchSummaryOptions, SUMMARIZATION_SYSTEM_PROMPT, calculate_context_tokens,
    collect_entries_for_branch_summary, compact, compute_file_lists, create_file_ops, estimate_context_tokens,
    estimate_tokens, extract_file_ops_from_message, find_cut_point, find_turn_start_index, format_file_operations,
    generate_branch_summary, generate_summary, get_last_assistant_usage, prepare_branch_entries, prepare_compaction,
    serialize_conversation, should_compact,
};
pub use datastore::Migration;
pub use datastore::{DatabaseSpec, ensure_database, ensure_databases, ensure_databases_once};
pub use elph_ai::{OnPayloadCallback, OnResponseCallback};
pub use elph_core::logger::{LogRotation, LoggingOptions};
pub use elph_core::{ensure_dirs, write_file_if_missing, write_json_file, write_private_file};
pub use goals::{Goal, GoalRuntime, GoalStatus, GoalStore, create_goal_tools};
pub use messages::{
    CustomMessageContent, bash_execution_to_text, create_branch_summary_message, create_compaction_summary_message,
    create_custom_message, default_convert_to_llm, default_convert_to_llm as convert_to_llm, default_convert_to_llm_fn,
    now_iso_timestamp,
};
#[cfg(feature = "extensions")]
pub use plugins::{
    ExtensionCommand, ExtensionManifest, ExtensionRegistry, ExtensionSlashResult, ExtensionsSettings,
    discover_manifests, extension_roots, global_extensions_dir, load_manifest, project_extensions_dir,
};
pub use prompt::encoding::{
    PromptEncodingConfig, PromptEncodingDelimiter, PromptEncodingMode, PromptEncodingTargets, ToonDecodeError,
    apply_to_tool_result, decode_toon_fence, encode_value, extract_json_value, parse_toon_fence,
};
pub use prompt::session_name::generate_session_name;
pub use prompt::{
    LoadPromptTemplatesResult, LoadSourcedPromptTemplatesResult, PromptTemplateDiagnostic,
    PromptTemplateDiagnosticCode, SourcedPromptTemplate, SourcedPromptTemplateDiagnostic,
    format_prompt_template_invocation, load_prompt_templates, load_sourced_prompt_templates, parse_command_args,
    substitute_args,
};
pub use runtime::event_stream::{AgentEventSink, AgentEventStream};
pub use runtime::local_env::LocalExecutionEnv;
pub use runtime::proxy::{ProxyAssistantMessageEvent, ProxyStreamOptions, stream_proxy};
pub use runtime::{agent_loop, agent_loop_continue, run_agent_loop, run_agent_loop_continue};
pub use runtime::{block_on, try_block_on};
pub use session::id::create_kalid;
pub use session::{
    BranchSummaryOptions, ContextEntryTransform, CustomEntryContextMessageProjector, CustomMessageEntryBlock,
    CustomMessageEntryContent, EVENTS_FILE, ForkEntriesOptions, ForkPosition, InMemorySessionCreateOptions,
    InMemorySessionOptions, InMemorySessionRepo, InMemorySessionStorage, SESSION_TREE_MIGRATIONS, SUMMARY_FILE,
    Session, SessionContext, SessionContextBuildOptions, SessionDirCreateOptions, SessionDirListOptions,
    SessionDirMetadata, SessionDirRepo, SessionDirRepoCreateOptions, SessionDirStorage, SessionError, SessionErrorCode,
    SessionMetadata, SessionModelRef, SessionStorage, SessionTreeEntry, TursoSessionMetadata, TursoSessionStorage,
    build_context_entries, build_session_context, build_session_context_with_options, create_session_id,
    create_timestamp, default_context_entry_transform, get_entries_to_fork, load_session_metadata, to_session,
};
pub use skills::{
    LoadSkillsResult, LoadSourcedSkillsResult, SkillDiagnostic, SkillDiagnosticCode, SourcedSkill,
    SourcedSkillDiagnostic, format_skill_invocation, format_skill_missing_args_notice, load_skills,
    load_skills_with_options, load_sourced_skills, load_sourced_skills_with_options, skill_args_validation_notice,
    skill_requires_arguments,
};
#[cfg(any(feature = "tools-edit-tools", feature = "tools-search"))]
pub use tools::create_all_tools;
#[cfg(any(feature = "tools-edit-tools", feature = "tools-search", feature = "tools-web"))]
pub use tools::create_all_tools_with_web;
#[cfg(feature = "tools-bash")]
pub use tools::create_bash_tool;
#[cfg(feature = "tools-collaboration")]
pub use tools::create_collaboration_tools;
#[cfg(feature = "tools-copy-path")]
pub use tools::create_copy_path_tool;
#[cfg(feature = "tools-create-dir")]
pub use tools::create_create_dir_tool;
#[cfg(feature = "tools-delete-path")]
pub use tools::create_delete_path_tool;
#[cfg(feature = "tools-edit-file")]
pub use tools::create_edit_file_tool;
#[cfg(feature = "tools-edit-tools")]
pub use tools::create_edit_tools;
#[cfg(feature = "tools-find-path")]
pub use tools::create_find_path_tool;
#[cfg(feature = "tools-grep")]
pub use tools::create_grep_tool;
pub use tools::create_list_available_tools;
#[cfg(feature = "tools-list-dir")]
pub use tools::create_list_dir_tool;
#[cfg(feature = "tools-move-path")]
pub use tools::create_move_path_tool;
#[cfg(feature = "tools-read-file")]
pub use tools::create_read_file_tool;
#[cfg(feature = "tools-search")]
pub use tools::create_search_tools;
#[cfg(feature = "tools-write-file")]
pub use tools::create_write_file_tool;
#[cfg(feature = "mcp")]
pub use tools::mcp::{
    Aes256Key, AuthStoreFile, AuthStorePathBuilder, DEFAULT_AUTH_FILE_NAME, DEFAULT_AUTH_KEY_FILE_NAME,
    DEFAULT_MAX_STRUCTURED_DETAIL_CHARS, DEFAULT_MAX_TOOL_RESULT_CHARS, DEFAULT_OAUTH_SCOPES,
    DEFAULT_OPERATION_TIMEOUT_SECS, ENC_PREFIX, FileCredentialStore, FileCredentialStoreBuilder, McpAuthConflictPolicy,
    McpAuthSource, McpAuthSourceReport, McpClient, McpClientService, McpConfig, McpConfigValidationError,
    McpConnectContext, McpEventBus, McpHttpConfig, McpLoadOptions, McpLoadReport, McpOAuthClientMeta,
    McpOAuthFlowOptions, McpOAuthFlowResult, McpPolicyAction, McpPolicyConfig, McpProbeResult, McpPromptDescriptor,
    McpResourceDescriptor, McpServerConfig, McpServerEvent, McpServerLoadReport, McpServerSession, McpSessionPool,
    McpStdioConfig, McpToolDescriptor, McpToolRegistry, PROBE_TIMEOUT, auth_store_path, call_stdio_tool,
    call_tool_for_server, clear_credentials, connect, connect_http, connect_stdio, connect_with_context, decrypt_async,
    decrypt_json_async, decrypt_string_async, decrypt_string_sync, default_auth_key_path, encrypt_async,
    encrypt_json_async, encrypt_string_async, encrypt_string_sync, expose_tool_name, has_stored_credentials,
    is_encrypted_value, list_tools, list_tools_for_server, mcp_result_to_agent, mcp_result_to_agent_with_limit,
    mcp_tool_requires_approval, parse_and_validate_mcp_config, parse_and_validate_mcp_config_async,
    parse_and_validate_server_config_json, parse_exposed_tool_name, parse_stdio_config, pattern_matches, probe_server,
    probe_server_with_auth, probe_stdio_server, resolve_oauth_access_token, resolve_remote_auth, run_oauth_flow,
    run_oauth_flow_with_scopes, shutdown_client, truncate_chars, validate_mcp_config, validate_mcp_config_semantic,
    validate_mcp_config_value, validate_server_config,
};
#[cfg(feature = "tools-web")]
pub use tools::{WebSearchEngine, WebSearchResult};
#[cfg(feature = "tools-web")]
pub use tools::{create_web_fetch_tool, create_web_search_tool, create_web_tools};
pub use tools::{echo_tool, simple_tool};
pub use types::*;
