//! Pi coding-agent port — session orchestration above `elph-agent`.

mod agents_load;
mod aside;
mod ask_user;
mod conflict_notice;
mod events;
pub(crate) mod goal_slash;
mod handover;
mod headless_status;
pub mod mcp_bootstrap;
pub(crate) mod mode_change;
pub mod model_registry;
mod overlays;
pub(crate) mod plan_files;
mod pretty_markdown;
pub mod prompt;
pub(crate) mod provider;
mod provider_catalog;
mod resource_loader;
mod run_mode;
mod runtime;
mod session;
mod session_continuity;
mod session_info_slash;
mod session_manager;
mod skills_load;
mod slash_commands;
mod slash_misc;
mod system_prompt_slash;
mod todo_hooks;
mod tool_policy;
mod tools_catalog;
mod tools_slash;
mod worker_runtime;
mod workspace_reload;

pub use agents_load::{AgentConflict, WorkspaceAgent, WorkspaceAgents};
pub use agents_load::{agent_dir_entries, ensure_global_agents_md};
pub use agents_load::{format_agent_conflict_notice, load_workspace_agents};
pub use aside::WORKER_INBOUND_PROMPT_PREFIX;
pub use aside::aside_answer;
pub use aside::extract_worker_payload_text;
pub use aside::spawn_aside;
pub use conflict_notice::{CrossKindConflict, TemplateConflict, format_name_conflicts};
pub use events::{AgentUiEvent, SubagentUiPhase, ToolApprovalChoice};
pub use events::{
    CONTINUE_META_LABEL, RETRY_CONTINUE_PROMPT, ToolApprovalRequest, UserQuestionOption, UserQuestionRequest,
    UserQuestionStep,
};
pub use events::{ModeChangeRequest, PlanConfirmationRequest, QueuedPromptItem, QueuedPromptKind};
pub use handover::{
    CODEX_HANDOVER_PROMPT_PREFIX, ClaudeHandover, CodexHandover, HANDOVER_PROMPT_PREFIX, HandoverError,
    HandoverSession, HandoverToolCall, HandoverToolResult, HandoverTurn, HandoverWarning, build_codex_handoff_prompt,
    build_handoff_prompt, claude_config_dir, codex_config_dir, discover_claude_sessions,
    discover_claude_sessions_with_config, discover_codex_sessions, discover_codex_sessions_with_config,
    read_claude_session, read_codex_session, resolve_claude_session, resolve_codex_session, slugify,
};
pub use mcp_bootstrap::discover_mcp_registry;
pub use model_registry::ModelSelection;
pub use model_registry::resolve_model;
pub use overlays::{
    list_model_select_items, list_session_select_items, list_tree_select_items, list_tree_select_items_with_leaf,
    parse_model_value,
};
pub use provider::{DEFAULT_MODEL_ID, DEFAULT_PROVIDER};
pub use provider::{is_known_provider, provider_api_key_env, provider_config, resolve_provider_and_model};
pub use provider_catalog::install_providers_dir;
pub use resource_loader::LoadResourcesResult;
pub use resource_loader::{format_resource_conflict_notice, format_resource_load_warnings, load_resources};
pub use run_mode::RunModeOptions;
pub use run_mode::{OutputFormat, parse_agent_mode, parse_effort, resolve_system_prompt_arg, run_non_interactive};
pub use runtime::CreateSessionOptions;
pub use runtime::create_coding_session_with_events;
pub use session::CodingAgentSession;
pub use session_info_slash::{rename_session_title, session_info_slash_message, session_title_for_rename};
pub use session_manager::{SessionManager, load_session_tree_jsonl};
pub use skills_load::SkillConflict;
pub use skills_load::{MAX_PALETTE_DESCRIPTION_CHARS, format_skill_conflict_notice, truncate_palette_description};
pub use skills_load::{parse_skill_slash, skill_slash_name};
pub use slash_commands::{OverlayCommand, SlashDispatch};
pub use slash_commands::{
    SlashArgCompletion, slash_arg_completions, slash_commands_for_palette, slash_palette_submit_on_enter,
    slash_unimplemented_message,
};
pub use slash_commands::{confetti_mode_from_args, dispatch_slash_command, format_help_message};
pub use slash_misc::{
    HOTKEYS_TEXT, changelog_text, clone_session_message, export_session_message, fork_session_message,
    import_session_from_jsonl, import_slash_message, path_command_argument, resume_list_message,
    settings_slash_message, tree_list_message, tree_slash_message, trust_slash_message, workers_slash_message,
};
pub use system_prompt_slash::system_prompt_slash_message;
pub use tool_policy::agent_mode_from_setting;
pub use tool_policy::from_agent_thinking;
pub use tool_policy::thinking_level_from_setting;
pub use tool_policy::to_agent_thinking;
pub use tools_slash::{discovery_tools_message, tools_slash_message};
pub use workspace_reload::{WorkspaceReloadReport, WorkspaceReloadRequest};
