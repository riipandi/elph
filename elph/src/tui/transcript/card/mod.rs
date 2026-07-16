//! Transcript card rendering: chrome, frames, per-kind cards, builder, sticky overlay.

mod builder;
mod chrome;
mod frame;
mod kinds;
mod sticky;
mod tool_format;

pub use builder::build_transcript_bubbles;
pub use sticky::transcript_sticky_overlay;

pub(crate) use chrome::{
    COLORED_CARD_GAP, COLORED_CARD_PAD, COLORED_CARD_PAD_H, FLUSH_CARD_GAP, FLUSH_CARD_PAD, THINKING_RESPONSE_GAP,
};
pub(crate) use kinds::tool_status_marker;
pub(crate) use tool_format::{format_tool_args_display, format_tool_output_display};
