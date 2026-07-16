//! Pi coding-agent port — session orchestration above `elph-agent`.

mod ask_user;
mod diagnostics;
mod events;
pub(crate) mod goal_slash;
mod model_registry;
mod overlays;
mod provider;
mod resource_loader;
mod run_mode;
mod runtime;
mod session;
mod session_manager;
mod skills_load;
mod slash_commands;
mod system_prompt;
mod tool_policy;
mod tools_catalog;

pub use events::{AgentUiEvent, ToolApprovalChoice, ToolApprovalRequest, UserQuestionOption, UserQuestionRequest};
pub use overlays::{list_model_select_items, list_session_select_items, list_tree_select_items};
pub use provider::{DEFAULT_MODEL_ID, DEFAULT_PROVIDER, provider_api_key_env, provider_config};
pub use resource_loader::{LoadResourcesResult, load_resources};
pub use run_mode::{RunModeOptions, run_non_interactive};
pub use runtime::{CreateSessionOptions, create_coding_session_with_events};
pub use session::CodingAgentSession;
pub use session_manager::SessionManager;
pub use skills_load::{
    SkillConflict, format_skill_conflict_notice, parse_skill_slash, skill_slash_name,
    truncate_skill_palette_description,
};
pub use slash_commands::{OverlayCommand, SlashDispatch, dispatch_slash_command, format_help_message};
pub use slash_commands::{slash_commands_for_palette, slash_unimplemented_message};
pub use tool_policy::agent_mode_from_setting;
