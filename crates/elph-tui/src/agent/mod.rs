//! Agent-facing SLT render helpers (Elph-parity UI).

mod assistant_message;
mod composer_view;
mod detail_block;
mod list_modal;
mod login_dialog;
mod model_selector;
mod oauth_selector;
mod plan_confirmation;
mod session_selector;
mod tool_approval;
mod tool_execution;
mod transcript_view;
mod tree_navigator;

pub use assistant_message::render_assistant_message;
pub use composer_view::{composer_demo_entries, render_composer_transcript, render_tool_block, render_user_card};
pub use detail_block::{
    CollapseState, detail_dot, detail_dot_color, format_detail_hint, render_detail_block, render_pipe_message,
};
pub use login_dialog::{AuthStatus, render_login_dialog};
pub use model_selector::{
    ModelSelectorAction, ModelSelectorState, handle_model_selector_input, model_overlay_slot, render_model_selector,
};
pub use oauth_selector::{
    OAuthSelectorAction, OAuthSelectorState, handle_oauth_selector_input, mock_oauth_providers, render_oauth_selector,
};
pub use plan_confirmation::{
    PlanConfirmationAction, PlanConfirmationChoice, PlanConfirmationState, handle_plan_confirmation_input,
    render_plan_confirmation,
};
pub use session_selector::{
    SessionSelectorAction, SessionSelectorState, handle_session_selector_input, render_session_selector,
    session_overlay_slot,
};
pub use tool_approval::{
    ToolApprovalAction, ToolApprovalChoice as TuiToolApprovalChoice, ToolApprovalState, handle_tool_approval_input,
    render_tool_approval,
};
pub use tool_execution::{render_tool_execution_card, render_tool_execution_list};
pub use transcript_view::render_transcript_view;
pub use tree_navigator::{
    TreeNavigatorAction, TreeNavigatorState, handle_tree_navigator_input, render_tree_navigator, tree_overlay_slot,
};
