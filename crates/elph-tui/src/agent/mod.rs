//! Agent-facing iocraft components (Pi-parity UI).

mod assistant_message;
mod login_dialog;
mod model_selector;
mod oauth_selector;
mod session_selector;
mod tool_execution;
mod transcript_view;

pub use assistant_message::{AssistantMessage, AssistantMessageProps};
pub use login_dialog::{AuthStatus, LoginDialog, LoginDialogProps};
pub use model_selector::{ModelSelector, ModelSelectorProps, model_overlay_slot};
pub use oauth_selector::{OAuthSelector, OAuthSelectorProps, mock_oauth_providers};
pub use session_selector::{SessionSelector, SessionSelectorProps, session_overlay_slot};
pub use tool_execution::{ToolExecutionCard, ToolExecutionCardProps, ToolExecutionList, ToolExecutionListProps};
pub use transcript_view::{TranscriptView, TranscriptViewProps};
