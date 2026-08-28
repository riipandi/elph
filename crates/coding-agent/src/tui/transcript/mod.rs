//! Scrollable transcript panel with sticky user prompts.

pub(crate) mod archive;
mod card;
pub mod ephemeral;
mod layout;
pub(crate) mod markdown;
mod panel;
pub(crate) mod retention;
mod types;

pub use archive::duration_from_tool_details;
pub use ephemeral::{
    AGENT_MODE_NOTICE_TTL, EphemeralBanner, EphemeralBannerGeneration, EphemeralBannerKind,
    FILE_PICKER_HIDDEN_NOTICE_KEY, MODEL_SET_NOTICE_KEY, agent_mode_banner, agent_mode_busy_banner, api_error_banner,
    clear_ephemeral_banner, clear_ephemeral_banner_if_generation, clipboard_notice_banner, expire_ephemeral_banner,
    file_picker_hidden_notice_text, model_set_notice_from_value, model_set_notice_text, prompt_copy_banner,
    prompt_copy_failed_banner, publish_ephemeral_banner, quit_busy_banner, select_mode_off_banner,
    select_mode_on_banner, theme_mode_banner, thinking_level_banner,
};
pub use panel::TranscriptPanel;
pub use retention::apply_transcript_retention;
pub use types::{
    LogDensity, QUIT_BUSY_NOTICE_KEY, TranscriptMessage, TranscriptStyle, set_log_density,
    toggle_latest_collapsible_detail,
};
