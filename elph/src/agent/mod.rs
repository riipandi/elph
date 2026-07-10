//! Pi coding-agent port — session orchestration above `elph-agent`.

mod events;
// Slash handlers are stubbed until commands ship; keep /goal wiring ready.
#[expect(dead_code)]
pub(crate) mod goal_slash;
mod model_registry;
mod overlays;
mod provider;
mod resource_loader;
mod run_mode;
mod runtime;
mod session;
mod session_manager;
mod slash_commands;
mod system_prompt;
mod tool_policy;

pub use events::{AgentUiEvent, ToolApprovalChoice};
pub use overlays::{list_model_select_items, list_session_select_items, list_tree_select_items};
pub use provider::{provider_api_key_env, provider_config};
pub use run_mode::{RunModeOptions, run_non_interactive};
pub use runtime::{CreateSessionOptions, create_coding_session_with_events};
pub use session::CodingAgentSession;
pub use session_manager::SessionManager;
pub use slash_commands::{SlashDispatch, dispatch_slash_command, slash_commands_for_palette, slash_stub_message};
pub use tool_policy::agent_mode_from_setting;
