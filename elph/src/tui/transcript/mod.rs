//! Scrollable transcript panel with sticky user prompts.

mod card;
pub mod ephemeral;
mod layout;
pub(crate) mod markdown;
mod panel;
mod types;

pub use ephemeral::{
    EphemeralBanner, EphemeralBannerGeneration, agent_mode_banner, agent_mode_busy_banner, api_error_banner,
    clear_ephemeral_banner, clear_ephemeral_banner_if_generation, clipboard_notice_banner, expire_ephemeral_banner,
    prompt_copy_banner, prompt_copy_failed_banner, publish_ephemeral_banner, quit_busy_banner, select_mode_off_banner,
    select_mode_on_banner, theme_mode_banner,
};
pub use panel::TranscriptPanel;
pub use types::{QUIT_BUSY_NOTICE_KEY, TranscriptMessage, TranscriptStyle, toggle_latest_collapsible_detail};
