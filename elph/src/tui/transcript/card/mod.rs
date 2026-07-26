//! Transcript card rendering: chrome, frames, per-kind cards, builder, sticky overlay.

mod builder;
mod chrome;
mod frame;
mod kinds;
mod sticky;
pub(crate) mod timestamp_layout;
mod toggle_ctx;
mod tool_format;

#[allow(unused_imports)] // full rebuild kept for tests/callers that need every bubble
pub use builder::{build_transcript_bubbles, build_transcript_bubbles_windowed};
pub use sticky::transcript_sticky_overlay;
pub use toggle_ctx::CollapsibleToggleCtx;

pub(crate) use chrome::{
    COLORED_CARD_GAP, COLORED_CARD_PAD, COLORED_CARD_PAD_H, FLUSH_CARD_GAP, FLUSH_CARD_PAD, LOG_ROW_GAP,
    THINKING_RESPONSE_GAP, TOOL_TO_RESPONSE_GAP,
};
pub(crate) use kinds::tool_status_marker;
pub(crate) use tool_format::{
    format_assistant_stream_body_display, format_thinking_body_display, format_tool_args_display,
    format_tool_output_display,
};
