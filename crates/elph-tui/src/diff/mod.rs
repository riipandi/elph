mod ansi;
mod autocomplete;
mod component;
mod content;
mod cursor;
mod diff_view;
mod editor;
mod fuzzy;
mod hardware_cursor;
mod image;
mod keybindings;
mod keys;
mod kill_ring;
mod loader;
mod markdown;
mod markdown_table;
mod overlay;
mod paste;
mod paste_burst;
mod render;
mod select_list;
mod settings_list;
mod stdin_buffer;
mod streaming_markdown;
mod terminal;
mod terminal_image;
mod text;
mod text_buffer;
mod text_edit;
mod tui;
mod undo_stack;

pub use ansi::{BOLD, DIM, ITALIC, RESET, STRIKE, StylePrefix, UNDERLINE, hyperlink};
pub use autocomplete::{AutocompletePopup, AutocompleteProvider, CombinedAutocompleteProvider, SlashCommand};
pub use component::{Container, Focusable, InputResult, Line, LineComponent, TextBlock};
pub use content::{
    ChangeType, DiffLine, InlineSegment, compute_side_by_side, count_added_removed, expand_tabs, expand_tabs_default,
    find_hunk_starts,
};
pub use cursor::CursorPosition;
pub use cursor::{CURSOR_MARKER, LINE_RESET, extract_and_strip_cursor};
pub use diff_view::DiffView;
pub use editor::{Editor, EditorTheme};
pub use fuzzy::{FuzzyMatch, fuzzy_filter, fuzzy_match};
pub use hardware_cursor::{apply_hardware_cursor, hardware_cursor_enabled};
pub use image::{Image, ImageOptions, ImageTheme};
pub use keybindings::{EditorAction, match_editor_action};
pub use loader::{CancellableLoader, Loader};
pub use markdown::{Markdown, MarkdownTheme, render_markdown_lines};
pub use markdown_table::{is_gfm_table_row, render_gfm_pipe_table, render_gfm_table_data};
pub use overlay::{
    OverlayAnchor, OverlayHandle, OverlayLayout, OverlayMargin, OverlayOptions, SizeValue, composite_line_at,
    resolve_layout,
};
pub(crate) use overlay::{OverlayEntry, composite_overlays};
pub use render::{RenderState, SYNC_BEGIN, SYNC_END, do_render, first_changed_line};
pub use select_list::{SelectItem, SelectList, SelectListTheme};
pub use settings_list::{SettingItem, SettingsList, SettingsListTheme};
pub use stdin_buffer::{InputEvent, StdinBuffer};
pub use streaming_markdown::{partition_streaming_markdown, render_streaming_markdown_lines};
pub use terminal::{CrosstermTerminal, RecordingTerminal, Terminal, open_tui_writer};
pub use terminal_image::{ImageProtocol, detect_image_protocol, encode_inline_image, png_dimensions};
pub use text::Text;
pub use tui::DiffTui;
