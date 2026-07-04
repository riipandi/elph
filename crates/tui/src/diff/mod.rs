mod ansi;
mod component;
mod content;
mod cursor;
mod diff_view;
mod editor;
mod fuzzy;
mod keybindings;
mod keys;
mod kill_ring;
mod loader;
mod markdown;
mod overlay;
mod paste_burst;
mod render;
mod select_list;
mod stdin_buffer;
mod terminal;
mod text;
mod tui;
mod undo_stack;

pub use ansi::{BOLD, DIM, ITALIC, RESET, STRIKE, StylePrefix, UNDERLINE, hyperlink};
pub use component::{Container, Focusable, InputResult, Line, LineComponent, TextBlock};
pub use content::{
    ChangeType, DiffLine, InlineSegment, compute_side_by_side, count_added_removed, expand_tabs, find_hunk_starts,
};
pub use cursor::CursorPosition;
pub use cursor::{CURSOR_MARKER, LINE_RESET, extract_and_strip_cursor};
pub use diff_view::DiffView;
pub use editor::{Editor, EditorTheme};
pub use fuzzy::{FuzzyMatch, fuzzy_filter, fuzzy_match};
pub use keybindings::{EditorAction, match_editor_action};
pub use loader::Loader;
pub use markdown::{Markdown, MarkdownTheme};
pub use overlay::{
    OverlayAnchor, OverlayHandle, OverlayLayout, OverlayMargin, OverlayOptions, SizeValue, composite_line_at,
    resolve_layout,
};
pub use render::{RenderState, SYNC_BEGIN, SYNC_END, do_render, first_changed_line};
pub use select_list::{SelectItem, SelectList, SelectListTheme};
pub use stdin_buffer::{InputEvent, StdinBuffer};
pub use terminal::{CrosstermTerminal, RecordingTerminal, Terminal, open_tui_writer};
pub use text::Text;
pub use tui::DiffTui;
