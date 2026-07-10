mod actions;
mod line_component;
mod render;
mod state;

use super::autocomplete::{AutocompletePopup, AutocompleteProvider};
use super::kill_ring::KillRing;
use super::paste_burst::PasteBurst;
use super::undo_stack::UndoStack;

/// Callback invoked when the editor submits.
pub type EditorSubmitCallback = Box<dyn FnMut(&str)>;
/// Callback invoked when editor text changes.
pub type EditorChangeCallback = Box<dyn FnMut(&str)>;

/// Theme colors for the diff editor chrome.
#[derive(Debug, Clone, Copy)]
pub struct EditorTheme {
    pub border: u8,
    pub text: u8,
    pub cursor: u8,
}

impl EditorTheme {
    pub fn dark() -> Self {
        Self {
            border: 240,
            text: 252,
            cursor: 252,
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct EditorSnapshot {
    text: String,
    cursor: usize,
}

/// Multi-line editor (`LineComponent` + `CURSOR_MARKER`).
pub struct Editor {
    pub(super) text: String,
    pub(super) cursor: usize,
    pub(super) padding_x: u16,
    pub(super) max_visible_rows: usize,
    pub(super) focused: bool,
    pub(super) scroll_row: usize,
    pub(super) visual_col_pref: Option<u16>,
    pub(super) theme: EditorTheme,
    pub(super) kill_ring: KillRing,
    pub(super) undo: UndoStack<EditorSnapshot>,
    pub(super) paste_burst: PasteBurst,
    pub(super) disable_submit: bool,
    pub(super) pastes: Vec<super::paste::CollapsedPaste>,
    pub(super) last_yank_len: usize,
    pub(super) last_width: u16,
    pub(super) cache_key: Option<(usize, u16, usize)>,
    pub(super) cache_lines: Vec<super::component::Line>,
    pub on_submit: Option<EditorSubmitCallback>,
    pub on_change: Option<EditorChangeCallback>,
    pub(super) autocomplete_provider: Option<Box<dyn AutocompleteProvider>>,
    pub(super) pending_autocomplete: Option<AutocompletePopup>,
}

impl Editor {
    pub fn new(theme: EditorTheme) -> Self {
        Self {
            text: String::new(),
            cursor: 0,
            padding_x: 1,
            max_visible_rows: 8,
            focused: false,
            scroll_row: 0,
            visual_col_pref: None,
            theme,
            kill_ring: KillRing::default(),
            undo: UndoStack::default(),
            paste_burst: PasteBurst::default(),
            disable_submit: false,
            pastes: Vec::new(),
            last_yank_len: 0,
            last_width: 80,
            cache_key: None,
            cache_lines: Vec::new(),
            on_submit: None,
            on_change: None,
            autocomplete_provider: None,
            pending_autocomplete: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::component::LineComponent;
    use super::super::cursor::CURSOR_MARKER;
    use super::*;

    #[test]
    fn inserts_and_renders_text() {
        let mut editor = Editor::new(EditorTheme::dark());
        editor.set_focused(true);
        editor.handle_input("hello");
        let lines = editor.render(40);
        assert!(!lines.is_empty());
        assert!(lines.iter().any(|l| l.contains("hello")));
    }

    #[test]
    fn emits_cursor_marker_when_focused() {
        let mut editor = Editor::new(EditorTheme::dark());
        editor.set_focused(true);
        editor.set_text("abc");
        editor.set_cursor(1);
        let lines = editor.render(40);
        let joined = lines.join("");
        assert!(joined.contains(CURSOR_MARKER));
    }
}
