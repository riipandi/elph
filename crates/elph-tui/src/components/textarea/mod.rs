//! Multiline prompt editor (tui-textarea style).
//!
//! One [`TextareaState`] buffer, one terminal hook, direct `Text` render — no controlled
//! [`TextInput`] round-trip.

mod component;
mod input;
mod layout;
mod state;

pub use component::PaletteKeyInput;
pub use component::Textarea;
pub use layout::TextareaLayout;
pub use layout::{
    compute_viewport_height, display_row_count, layout_cursor_for_viewport, layout_textarea, layout_textarea_measured,
};
pub use layout::{logical_line_count, visible_row_count};
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
    /// When true, Tab/→/Enter are left to the slash palette (no caret move or submit).
    pub slash_palette_active: Option<Ref<bool>>,
    /// When true, Tab/→/Enter are left to the `@` file picker (no caret move or submit).
    pub file_picker_active: Option<Ref<bool>>,
    /// Optional ANSI-styled text for rendering (logical buffer unchanged).
    pub styled_content: Option<Ref<String>>,
    /// Mirror of the editor caret offset for parent palettes (`@` mentions).
    pub live_cursor: Option<Ref<usize>>,
    /// Parent sets true after palette completion to force a one-shot draft sync.
    pub force_palette_sync: Option<Ref<bool>>,
    /// Parent sets `true` to clear the live buffer (e.g. Ctrl+C while idle).
    pub force_clear: Option<Ref<bool>>,
    /// When enabled, `/`, `!`, and `!!` are shown in the prefix column instead of the buffer.
    pub prefix_config: Option<PromptPrefixConfig>,
    pub input_prefix_kind: Option<Ref<InputPrefixKind>>,
    pub scrollbar_style: Option<ScrollbarStyle>,
    pub theme: Option<UiTheme>,
    /// When true, plain `Enter` calls [`Self::on_submit`] (Shift+Enter / Ctrl+J still insert newlines).
    pub submit_on_enter: bool,
    pub on_submit: HandlerMut<'static, String>,
    /// Plain `Esc` (no CSI-u prefix) — e.g. blur the editor for transcript scroll.
    pub on_escape: HandlerMut<'static, ()>,
    /// `@` file picker keys — runs with a flushed editor buffer before the default handler.
    pub on_file_picker_key: HandlerMut<'static, PaletteKeyInput>,
    /// Set to true by [`Self::on_file_picker_key`] when the key was fully handled.
    pub file_picker_key_handled: Option<Ref<bool>>,
    /// Flushed editor buffer for parent key handlers (updated each render and input event).
    pub prompt_editor_mirror: Option<Ref<(String, usize)>>,
    /// One-shot clipboard toast request (plain `y` selection yank). Shell drains into the ephemeral banner.
    pub clipboard_toast: Option<State<Option<crate::clipboard::ClipboardNotice>>>,
    /// Directory for staged clipboard images (`APP_DATA/attachments`).
    pub image_attachment_dir: Option<std::path::PathBuf>,
    /// Pending image files keyed by the atomic markers in the editor.
    pub image_attachments: Option<Ref<Vec<ImageAttachment>>>,
    /// Whether the active model accepts image input.
    pub supports_images: bool,
}

use crate::clipboard::ImageAttachment;
use crate::components::scroll_bar::ScrollbarStyle;
use crate::components::theme::UiTheme;
use crate::input_prefix::{InputPrefixKind, PromptPrefixConfig};
use iocraft::prelude::*;
