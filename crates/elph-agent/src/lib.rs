//! App-agnostic agent runtime: loop, tools, sessions, MCP, and harness.
//!
//! # Public contract
//!
//! Prefer crate-root types [`Agent`], [`AgentOptions`], [`HostIdentity`],
//! and the feature-gated tool constructors. Session orchestration lives at
//! [`harness`]; MCP at [`mcp`] (feature `mcp`); SQL schemas at [`session`].
//! Adapter internals and TypeScript-port helpers (`get_or_throw`,
//! `get_or_undefined`) are not a stability promise.
//!
//! Set [`HostIdentity`] on [`AgentOptions`] (env prefix for
//! `{PREFIX}_PROMPT_ENCODING*`). Pass the same prefix to MCP
//! [`mcp::load_or_create_master_key_with_prefix`] and logging [`AgentBuilder::env_prefix`].
//! Identity is not process-global.
//!
//! MSRV is Rust **1.89** (edition 2024). Cargo features: `mcp`, `builtin-tools`,
//! `extensions`, `prompt-templates`, `tracing`, `backend-turso`; bundle with `full`.
//!
//! Consumer notes: <https://github.com/riipandi/elph/blob/main/docs/elph-agent.md>
//!
//! Rust port of [@earendil-works/pi-agent](https://github.com/earendil-works/pi/tree/main/packages/agent).

#![cfg_attr(docsrs, feature(doc_cfg))]

pub mod agent;
pub mod builder;
pub mod compaction;
#[cfg(feature = "backend-turso")]
pub mod datastore;
pub mod fs;
pub mod goals;
pub mod logger;
pub mod messages;

pub mod collaboration;
pub mod exec;
#[cfg(feature = "extensions")]
pub mod plugins;
pub mod prompt;

pub mod runtime;

pub mod session;
pub mod session_summary;
pub mod skills;
pub mod todos;
pub mod turns;
pub mod workers;

pub mod tools;
pub mod trace;
pub mod types;
pub mod utils;

#[cfg(unix)]
pub use crate::exec::pty::{PtySize, open_pty};
pub use crate::exec::{ExecError, ExecErrorCode, ShellConfig, exec_shell_command, resolve_shell};
pub use agent::default_model;
pub use agent::harness;
pub use agent::subagent::AgentControl;
#[cfg(feature = "backend-turso")]
pub use agent::subagent::AgentGraphStore;
pub use agent::subagent::AgentRegistry;
pub use agent::subagent::SubagentBootstrap;
pub use agent::subagent::SubagentEventForwarder;
#[cfg(feature = "backend-turso")]
pub use agent::subagent::SubagentHarness;
pub use agent::subagent::SubagentInfo;
pub use agent::subagent::SubagentLimits;
pub use agent::subagent::SubagentOutput;
pub use agent::subagent::SubagentSpawnConfig;
pub use agent::subagent::SubagentStatus;
pub use agent::subagent::generate_agent_name;
pub use agent::subagent::subagent_persist_event;
pub use agent::{Agent, AgentListener, AgentOptions, AgentSubscription, PartialAgentState};
pub use builder::{AgentBuilder, AgentInit, BuiltinToolsBuilder};
pub use collaboration::ToolExposurePolicy;
pub use collaboration::assistant_message_text;
pub use collaboration::default_exploration_tools;
pub use collaboration::extract_proposed_plan;
pub use collaboration::filter_active_tools;
pub use collaboration::implement_prompt;
pub use collaboration::is_collaboration_tool;
pub use collaboration::is_exploration_tool;
pub use collaboration::is_mcp_read_only_bridge_tool;
pub use collaboration::is_mcp_tool;
pub use collaboration::is_mutating_tool;
pub use collaboration::is_plan_exposed_tool;
pub use collaboration::is_plan_mode_tool;
pub use collaboration::is_plan_workspace_mutating_tool;
pub use collaboration::is_read_only_mcp_tool;
pub use collaboration::plan_mode_block_reason;
pub use collaboration::plan_mode_blocks_tool;
pub use collaboration::{CollaborationMode, PlanConfirmationChoice, PlanModeState, PlanModeTracker};
pub use compaction::BranchPreparation;
pub use compaction::BranchSummaryDetails;
pub use compaction::CollectEntriesResult;
pub use compaction::CompactionDetails;
pub use compaction::CompactionPreparation;
pub use compaction::CompactionResult;
pub use compaction::CompactionSettings;
pub use compaction::ContextUsageEstimate;
pub use compaction::CutPointResult;
pub use compaction::FileOperations;
pub use compaction::GenerateBranchSummaryOptions;
pub use compaction::SUMMARIZATION_SYSTEM_PROMPT;
pub use compaction::calculate_context_tokens;
pub use compaction::collect_entries_for_branch_summary;
pub use compaction::compact;
pub use compaction::compute_file_lists;
pub use compaction::create_file_ops;
pub use compaction::estimate_context_tokens;
pub use compaction::estimate_tokens;
pub use compaction::estimate_tokens_with_system_prompt;
pub use compaction::extract_file_ops_from_message;
pub use compaction::find_cut_point;
pub use compaction::find_turn_start_index;
pub use compaction::format_file_operations;
pub use compaction::generate_branch_summary;
pub use compaction::generate_summary;
pub use compaction::get_last_assistant_usage;
pub use compaction::prepare_branch_entries;
pub use compaction::prepare_compaction;
pub use compaction::serialize_conversation;
pub use compaction::should_compact;
#[cfg(feature = "backend-turso")]
pub use datastore::DatabaseSpec;
#[cfg(feature = "backend-turso")]
pub use datastore::{ensure_database, ensure_databases, ensure_databases_once};
pub use elph_ai::{OnPayloadCallback, OnResponseCallback};
pub use fs::{ensure_dirs, write_file_if_missing, write_json_file, write_private_file};
pub use goals::{BUDGET_LIMIT_PROMPT_PREFIX, CONTINUATION_PROMPT_PREFIX};
pub use goals::{Goal, GoalStatus};
#[cfg(feature = "backend-turso")]
pub use goals::{GoalRuntime, GoalStatusHook, GoalStore, create_goal_tools, create_goal_tools_with_hook};
pub use logger::{LogRotation, LoggingOptions, LoggingOptionsBuilder, LoggingSettings};
pub use messages::CustomMessageContent;
pub use messages::create_branch_summary_message;
pub use messages::create_compaction_summary_message;
pub use messages::create_custom_message;
pub use messages::default_convert_to_llm;
pub use messages::default_convert_to_llm as convert_to_llm;
pub use messages::default_convert_to_llm_fn;
pub use messages::now_date_with_offset;
pub use messages::now_iso_timestamp;
pub use messages::shell_exec_execution_to_text;
#[cfg(feature = "extensions")]
pub use plugins::{ExtensionCommand, ExtensionManifest, ExtensionRegistry, ExtensionSlashResult, ExtensionsSettings};
#[cfg(feature = "extensions")]
pub use plugins::{discover_manifests, extension_roots, global_extensions_dir, load_manifest, project_extensions_dir};
pub use prompt::LoadPromptTemplatesResult;
pub use prompt::LoadSourcedPromptTemplatesResult;
pub use prompt::PromptTemplateDiagnostic;
pub use prompt::PromptTemplateDiagnosticCode;
pub use prompt::SourcedPromptTemplate;
pub use prompt::SourcedPromptTemplateDiagnostic;
pub use prompt::builtin::plan_mode_reentry_prompt;
pub use prompt::builtin::session_name::extract_conversation_for_naming;
pub use prompt::builtin::session_name::sanitize_session_name;
pub use prompt::encoding::PromptEncodingConfig;
pub use prompt::encoding::PromptEncodingDelimiter;
pub use prompt::encoding::PromptEncodingMode;
pub use prompt::encoding::PromptEncodingTargets;
pub use prompt::encoding::ToonDecodeError;
pub use prompt::encoding::apply_to_tool_result;
pub use prompt::encoding::decode_toon_fence;
pub use prompt::encoding::encode_value;
pub use prompt::encoding::extract_json_value;
pub use prompt::encoding::parse_toon_fence;
pub use prompt::format_prompt_template_invocation;
pub use prompt::load_prompt_templates;
pub use prompt::load_sourced_prompt_templates;
pub use prompt::parse_command_args;
pub use prompt::session_name::generate_session_name;
pub use prompt::session_name::generate_session_name_with_prompts;
pub use prompt::substitute_args;
pub use prompt::{DEFAULT_SYSTEM_PROMPT, resolve_system_prompt_text};
#[cfg(feature = "prompt-templates")]
pub use prompt::{
    PromptAssemblyMode, PromptRenderError, SystemPromptBuildError, SystemPromptBuilder, SystemPromptTemplateContext,
    ToolByKindContext, ToolNamesContext, format_project_context, render_base_template, tool_names_context,
};
pub use runtime::event_stream::{AgentEventSink, AgentEventStream};
pub use runtime::local_env::LocalExecutionEnv;
pub use runtime::proxy::stream_proxy;
pub use runtime::proxy::{ProxyAssistantMessageEvent, ProxyStreamOptions};
pub use runtime::{agent_loop, agent_loop_continue, run_agent_loop, run_agent_loop_continue};
pub use runtime::{block_on, try_block_on, try_block_on_detached};
pub use session::BranchSummaryOptions;
pub use session::ContextEntryTransform;
pub use session::CustomEntryContextMessageProjector;
pub use session::CustomMessageEntryBlock;
pub use session::CustomMessageEntryContent;
pub use session::ForkEntriesOptions;
pub use session::ForkPosition;
pub use session::InMemorySessionCreateOptions;
pub use session::InMemorySessionOptions;
pub use session::InMemorySessionRepo;
pub use session::InMemorySessionStorage;
pub use session::Migration;
pub use session::Session;
pub use session::SessionContext;
pub use session::SessionContextBuildOptions;
pub use session::SessionDirCreateOptions;
pub use session::SessionDirListOptions;
pub use session::SessionDirMetadata;
pub use session::SessionDirRepo;
pub use session::SessionDirRepoCreateOptions;
pub use session::SessionDirStorage;
pub use session::SessionError;
pub use session::SessionErrorCode;
pub use session::SessionMetadata;
pub use session::SessionModelRef;
pub use session::SessionStorage;
pub use session::SessionTreeEntry;
#[cfg(feature = "backend-turso")]
pub use session::TursoSessionCreateOptions;
#[cfg(feature = "backend-turso")]
pub use session::TursoSessionListOptions;
#[cfg(feature = "backend-turso")]
pub use session::TursoSessionMetadata;
#[cfg(feature = "backend-turso")]
pub use session::TursoSessionRepo;
#[cfg(feature = "backend-turso")]
pub use session::TursoSessionRepoCreateOptions;
#[cfg(feature = "backend-turso")]
pub use session::TursoSessionStorage;
pub use session::build_context_entries;
pub use session::build_session_context;
pub use session::build_session_context_with_options;
pub use session::create_session_id;
pub use session::create_timestamp;
pub use session::create_worker_id;
pub use session::create_worker_msg_id;
pub use session::default_context_entry_transform;
pub use session::derive_session_context_state;
pub use session::get_entries_to_fork;
pub use session::id::create_kalid;
pub use session::load_durable_state;
pub use session::load_session_metadata;
pub use session::reconcile_session;
pub use session::reduce_durable_state;
pub use session::repair_unanswered_tool_calls;
pub use session::to_session;
pub use session::{DurableHarnessState, OperationKind, OperationOutcome, QueueKind, RecoveryReport};
#[cfg(feature = "backend-turso")]
pub use session::{
    RetentionPolicy, SessionGcReport, list_session_gc_rows, run_full_session_gc, run_session_gc, set_session_pinned,
};
pub use session_summary::SessionSummary;
#[cfg(feature = "backend-turso")]
pub use session_summary::SessionSummaryStore;
#[cfg(feature = "backend-turso")]
pub use session_summary::create_session_summary_tool;
pub use skills::LoadSkillsResult;
pub use skills::LoadSourcedSkillsResult;
pub use skills::SkillDiagnostic;
pub use skills::SkillDiagnosticCode;
pub use skills::SourcedSkill;
pub use skills::SourcedSkillDiagnostic;
pub use skills::format_skill_invocation;
pub use skills::format_skill_missing_args_notice;
pub use skills::load_skills;
pub use skills::load_skills_with_options;
pub use skills::load_sourced_skills;
pub use skills::load_sourced_skills_with_options;
pub use skills::skill_args_validation_notice;
pub use skills::skill_requires_arguments;
#[cfg(feature = "backend-turso")]
pub use todos::{
    TodoHook, TodoStore, TodoUpdate, auto_close_done_todos, create_todo_tools, create_todo_tools_with_hook,
};
pub use todos::{TodoItem, TodoStatus, WorkTracker};
#[cfg(any(feature = "tools-edit", feature = "tools-search"))]
pub use tools::create_all_tools;
#[cfg(any(feature = "tools-edit", feature = "tools-search", feature = "tools-web"))]
pub use tools::create_all_tools_with_web;
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
#[cfg(feature = "tools-edit")]
pub use tools::create_edit_tools;
#[cfg(feature = "tools-find-path")]
pub use tools::create_find_path_tool;
#[cfg(feature = "tools-grep")]
pub use tools::create_grep_tool;
pub use tools::create_list_available_tools;
#[cfg(feature = "tools-list-dir")]
pub use tools::create_list_dir_tool;
pub use tools::create_list_skills_tool;
#[cfg(feature = "tools-move-path")]
pub use tools::create_move_path_tool;
#[cfg(feature = "tools-read-file")]
pub use tools::create_read_file_tool;
#[cfg(feature = "tools-search")]
pub use tools::create_search_tools;
#[cfg(feature = "tools-write-file")]
pub use tools::create_write_file_tool;
#[cfg(feature = "mcp")]
pub use tools::mcp;
#[cfg(feature = "tools-web")]
pub use tools::{WebSearchEngine, WebSearchResult};
#[cfg(feature = "tools-shell-exec")]
pub use tools::{
    cancel_background_task, create_shell_exec_tool, list_background_tasks, normalize_shell_exec_args,
    strip_redundant_cd_prefix,
};
#[cfg(feature = "tools-shell-use")]
pub use tools::{close_shell_use_sessions, create_shell_use_tool, shell_use_open_sessions};
#[cfg(feature = "tools-web")]
pub use tools::{create_web_extract_tool, create_web_fetch_tool, create_web_search_tool, create_web_tools};
pub use tools::{echo_tool, simple_tool};
#[cfg(feature = "backend-turso")]
pub use turns::TurnStore;
pub use turns::{TurnRecord, TurnStatus, TurnUsage};
pub use types::*;
#[cfg(feature = "backend-turso")]
pub use workers::{
    FileLeaseStore, LeaseConflict, LeaseError, MailboxStore, SessionLease, SessionLeaseStore, WorkerRegistry,
    WorkerToolContext, create_intercom_tools, create_worker_tools,
};
pub use workers::{
    LiveWorker, MessageKind, MessageStatus, PathClaimContext, SharedPathClaim, WorkerMessage, WorkerRecord,
    WorkerStatus, normalize_claim_path,
};
