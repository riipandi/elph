//! Scrollable transcript panel with sticky user prompts.

mod card;
pub mod ephemeral;
mod layout;
pub(crate) mod markdown;
mod panel;
mod types;

pub use ephemeral::{
    AGENT_MODE_NOTICE_KEY, agent_mode_notice_expired, next_agent_mode_notice_deadline, remove_ephemeral_notice,
    show_agent_mode_notice,
};
pub use panel::TranscriptPanel;
pub use types::{TranscriptMessage, TranscriptStyle};
