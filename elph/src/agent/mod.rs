//! Pi coding-agent port — session orchestration above `elph-agent`.

mod ask_user;
mod diagnostics;
mod events;
pub(crate) mod goal_slash;
pub mod mcp_bootstrap;
mod model_registry;
mod overlays;
mod prompt;
mod provider;
mod resource_loader;
mod run_mode;
mod runtime;
mod session;
mod session_manager;
mod skills_load;
mod slash_commands;
mod system_prompt_slash;
mod tool_policy;
mod tools_catalog;
mod tools_slash;

pub use events::{AgentUiEvent, SubagentUiPhase, ToolApprovalChoice};
pub use events::{ToolApprovalRequest, UserQuestionOption, UserQuestionRequest, UserQuestionStep};
pub use mcp_bootstrap::{discover_mcp_registry, wire_mcp_into_session};
pub use overlays::{list_model_select_items, list_session_select_items, list_tree_select_items, parse_model_value};
pub use provider::{DEFAULT_MODEL_ID, DEFAULT_PROVIDER};
pub use provider::{provider_api_key_env, provider_config, resolve_provider_and_model};
pub use resource_loader::LoadResourcesResult;
pub use resource_loader::load_resources;
pub use run_mode::RunModeOptions;
pub use run_mode::run_non_interactive;
pub use runtime::CreateSessionOptions;
pub use runtime::create_coding_session_with_events;
pub use session::CodingAgentSession;
pub use session_manager::SessionManager;
pub use skills_load::SkillConflict;
pub use skills_load::{format_skill_conflict_notice, truncate_skill_palette_description};
pub use skills_load::{parse_skill_slash, skill_slash_name};
pub use slash_commands::{OverlayCommand, SlashDispatch};
pub use slash_commands::{
    SlashArgCompletion, slash_arg_completions, slash_commands_for_palette, slash_palette_submit_on_enter,
    slash_unimplemented_message,
};
pub use slash_commands::{confetti_mode_from_args, dispatch_slash_command, format_help_message};
pub use system_prompt_slash::system_prompt_slash_message;
pub use tool_policy::agent_mode_from_setting;
pub use tools_slash::tools_slash_message;
