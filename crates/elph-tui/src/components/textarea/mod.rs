//! Multiline prompt editor (tui-textarea style).
//!
//! One [`TextareaState`] buffer, one terminal hook, direct `Text` render — no controlled
//! [`TextInput`] round-trip.

mod component;
mod input;
mod layout;
mod state;

pub use component::Textarea;
pub use layout::{
    TextareaLayout, compute_viewport_height, display_row_count, layout_cursor_for_viewport, layout_textarea,
    layout_textarea_measured, logical_line_count, visible_row_count,
};
pub use state::TextareaState;

/// Props for [`Textarea`].
#[derive(Default, Props)]
pub struct TextareaProps {
    pub width: u16,
    /// Minimum visible rows. Defaults to 1 when unset or zero.
    pub min_height: u16,
    /// Maximum visible rows before clipping and showing a scrollbar. Unset = grow without limit.
    pub max_height: Option<u16>,
    pub initial_value: String,
    pub has_focus: bool,
    pub text_color: Option<Color>,
    pub cursor_color: Option<Color>,
    pub value: Option<State<String>>,
    /// Live editor buffer mirror for parents that do not receive per-keystroke `value` updates.
    pub live_draft: Option<Ref<String>>,
    /// When false, omits the inner border (for embedding in a parent chrome).
    pub show_border: Option<bool>,
    /// Set by the parent on submit so plain Enter's ghost `\n` is dropped, not the next keystroke.
    pub suppress_enter_newline: Option<Ref<bool>>,
    pub scrollbar_style: Option<ScrollbarStyle>,
    /// When true, plain `Enter` calls [`Self::on_submit`] (Shift+Enter / Ctrl+J still insert newlines).
    pub submit_on_enter: bool,
    pub on_submit: HandlerMut<'static, String>,
}

use crate::components::scroll_bar::ScrollbarStyle;
use iocraft::prelude::*;
