//! Terminal UI components and helpers for Elph agent applications.

pub mod components;
pub mod diff;
pub mod prompt;
pub mod terminal;
pub mod theme;
pub mod utils;

pub use components::{Label, LabelProps, frame};
pub use diff::{
    CURSOR_MARKER, ChangeType, Container as DiffContainer, CrosstermTerminal, CursorPosition, DiffLine, DiffTui,
    DiffView, Editor, EditorAction, EditorTheme, FuzzyMatch, InlineSegment, InputEvent, InputResult, LineComponent,
    Loader, Markdown, MarkdownTheme, OverlayAnchor, OverlayHandle, OverlayMargin, OverlayOptions, RESET,
    RecordingTerminal, RenderState, SelectItem, SelectList, SelectListTheme, SizeValue, StdinBuffer, Terminal, Text,
    TextBlock, composite_line_at, compute_side_by_side, count_added_removed, do_render, extract_and_strip_cursor,
    find_hunk_starts, first_changed_line, fuzzy_filter, fuzzy_match, hyperlink, match_editor_action, open_tui_writer,
    resolve_layout,
};
pub use prompt::{
    AgentMode, ChatStream, ChatStreamProps, DEFAULT_LINE_SCROLL_STEP, EditAction, MacEditAction, PAGE_SCROLL_VIEWPORT,
    PromptInput, PromptInputProps, PromptTranscript, PromptTranscriptProps, edit_action, is_force_quit_key,
    is_interrupt_key, is_mode_cycle_key, is_newline_key, is_prompt_newline_key, is_quit_command, is_submit_key,
    is_theme_toggle_key, mac_edit_action,
};
pub use terminal::{
    SigintReceiver, disable_keyboard_enhancement, enable_keyboard_enhancement, key_combination, sigint_channel,
};
pub use theme::{Theme, ThemeMode};
pub use utils::{
    TAB_STOP, char_display_width, pad_lines, str_display_width, truncate_to_width, wrap_ansi_line, wrap_ansi_text,
    wrap_text,
};
