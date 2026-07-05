use std::time::Instant;

use crate::prompt::prompt_buffer::{PromptBuffer, char_display_width, expand_for_display};
use crate::prompt::prompt_edit::{
    char_left, char_right, delete_char_backward, delete_char_forward, delete_to_line_end, delete_to_line_start,
    delete_word_backward, delete_word_forward, line_end, line_start, word_left, word_right,
};
use crate::prompt::prompt_paste::{
    CollapsedPaste, PASTE_COLLAPSE_MIN_CHARS, PASTE_COLLAPSE_MIN_LINES, adjust_pastes_for_delete, expand_paste_markers,
    line_count, normalize_paste_text, paste_block_range, reconcile_paste_offsets, remove_paste_block_and_adjust,
    shift_paste_offsets_for_insert, should_collapse_paste,
};
use crate::utils::{pad_lines, truncate_to_width_no_ellipsis};

use super::ansi::RESET as ANSI_RESET;
use super::ansi::{self, styled};
use super::autocomplete::{AutocompletePopup, AutocompleteProvider};
use super::component::{InputResult, Line, LineComponent};
use super::cursor::CURSOR_MARKER;
use super::keybindings::{EditorAction, match_editor_action};
use super::kill_ring::KillRing;
use super::paste_burst::PasteBurst;
use super::undo_stack::UndoStack;

const REVERSE_VIDEO: &str = "\x1b[7m";

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
struct EditorSnapshot {
    text: String,
    cursor: usize,
}

/// Multi-line editor with pi-tui semantics (`LineComponent` + `CURSOR_MARKER`).
pub struct Editor {
    text: String,
    cursor: usize,
    padding_x: u16,
    max_visible_rows: usize,
    focused: bool,
    scroll_row: usize,
    visual_col_pref: Option<u16>,
    theme: EditorTheme,
    kill_ring: KillRing,
    undo: UndoStack<EditorSnapshot>,
    paste_burst: PasteBurst,
    disable_submit: bool,
    pastes: Vec<CollapsedPaste>,
    last_yank_len: usize,
    last_width: u16,
    cache_key: Option<(String, usize, u16, usize)>,
    cache_lines: Vec<Line>,
    pub on_submit: Option<EditorSubmitCallback>,
    pub on_change: Option<EditorChangeCallback>,
    autocomplete_provider: Option<Box<dyn AutocompleteProvider>>,
    pending_autocomplete: Option<AutocompletePopup>,
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

    pub fn set_autocomplete_provider(&mut self, provider: Box<dyn AutocompleteProvider>) {
        self.autocomplete_provider = Some(provider);
    }

    pub fn take_autocomplete_popup(&mut self) -> Option<AutocompletePopup> {
        self.pending_autocomplete.take()
    }

    fn token_before_cursor(&self) -> String {
        let before = &self.text[..self.cursor.min(self.text.len())];
        before.split_whitespace().next_back().unwrap_or(before).to_string()
    }

    fn open_path_autocomplete(&mut self) {
        let Some(provider) = self.autocomplete_provider.as_ref() else {
            return;
        };
        let token = self.token_before_cursor();
        let cwd = std::env::current_dir().unwrap_or_else(|_| ".".into());
        let paths = provider.complete_path(&token, &cwd);
        if !paths.is_empty() {
            self.pending_autocomplete = Some(AutocompletePopup::paths(paths));
        }
    }

    fn open_slash_autocomplete(&mut self) {
        let Some(provider) = self.autocomplete_provider.as_ref() else {
            return;
        };
        let token = self.token_before_cursor();
        let filter = token.trim_start_matches('/');
        self.pending_autocomplete = Some(AutocompletePopup::slash_commands(provider.slash_commands(), filter));
    }

    pub fn with_max_visible_rows(mut self, rows: usize) -> Self {
        self.max_visible_rows = rows.max(1);
        self
    }

    pub fn set_padding_x(&mut self, padding_x: u16) {
        self.padding_x = padding_x;
        self.invalidate();
    }

    pub fn set_disable_submit(&mut self, disabled: bool) {
        self.disable_submit = disabled;
    }

    pub fn get_text(&self) -> &str {
        &self.text
    }

    pub fn get_expanded_text(&self) -> String {
        expand_paste_markers(&self.text, &self.pastes)
    }

    pub fn set_cursor(&mut self, offset: usize) {
        self.cursor = offset.min(self.text.len());
        self.invalidate();
    }

    pub fn set_text(&mut self, text: impl Into<String>) {
        self.undo.clear();
        self.push_undo();
        self.text = text.into();
        self.cursor = self.text.len().min(self.cursor);
        self.notify_change();
        self.invalidate();
    }

    pub fn insert_text_at_cursor(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        self.push_undo();
        self.insert_normalized(normalize_paste_text(text), 40);
        self.notify_change();
        self.invalidate();
    }

    fn notify_change(&mut self) {
        if let Some(cb) = &mut self.on_change {
            cb(&self.text);
        }
    }

    fn snapshot(&self) -> EditorSnapshot {
        EditorSnapshot {
            text: self.text.clone(),
            cursor: self.cursor,
        }
    }

    fn push_undo(&mut self) {
        const MAX_UNDO_DEPTH: usize = 200;
        if self.undo.len() >= MAX_UNDO_DEPTH {
            self.undo.clear();
        }
        self.undo.push(self.snapshot());
    }

    fn restore(&mut self, snap: EditorSnapshot) {
        self.text = snap.text;
        self.cursor = snap.cursor.min(self.text.len());
        self.notify_change();
        self.invalidate();
    }

    fn content_width(&self, width: u16) -> usize {
        width.saturating_sub(self.padding_x.saturating_mul(2)).max(1) as usize
    }

    fn layout(&self, width: u16) -> PromptBuffer {
        PromptBuffer::new(&self.text, self.content_width(width))
    }

    fn ensure_cursor_visible(&mut self, width: u16) {
        let buffer = self.layout(width);
        let (row, _) = buffer.row_column_for_offset(self.cursor);
        let row = row as usize;
        if row < self.scroll_row {
            self.scroll_row = row;
        } else if row >= self.scroll_row + self.max_visible_rows {
            self.scroll_row = row + 1 - self.max_visible_rows;
        }
    }

    fn render_content_line(&self, buffer: &PromptBuffer, row_idx: usize, width: usize) -> String {
        let rows = buffer.rows();
        if row_idx >= rows.len() {
            return String::new();
        }
        let row = &rows[row_idx];
        let slice = &self.text[row.offset..row.offset + row.len];
        let display = expand_for_display(slice);
        let (cursor_row, cursor_col) = buffer.row_column_for_offset(self.cursor);
        let is_cursor_row = row_idx == cursor_row as usize;

        if !is_cursor_row || !self.focused {
            return truncate_to_width_no_ellipsis(&styled(&ansi::fg(self.theme.text), &display), width);
        }

        let col = cursor_col as usize;
        let before = truncate_to_width_no_ellipsis(
            &styled(&ansi::fg(self.theme.text), &slice_at_display_col(&display, 0, col)),
            width,
        );
        let after_start = slice_at_display_col_offset(slice, col);
        let cursor_char = self
            .text
            .get(after_start..)
            .and_then(|s| s.chars().next())
            .unwrap_or(' ');
        let cursor_cell = format!("{CURSOR_MARKER}{REVERSE_VIDEO}{cursor_char}{ANSI_RESET}");
        let after_text = self.text.get(after_start..).unwrap_or("");
        let after_char_len = cursor_char.len_utf8();
        let after = &after_text[after_char_len.min(after_text.len())..];
        let after_display = expand_for_display(after);
        let after_styled = styled(&ansi::fg(self.theme.text), &after_display);
        truncate_to_width_no_ellipsis(&format!("{before}{cursor_cell}{after_styled}"), width)
    }

    fn build_lines(&mut self, width: u16) -> Vec<Line> {
        self.ensure_cursor_visible(width);
        let buffer = self.layout(width);
        let rows = buffer.rows();
        let content_width = self.content_width(width);
        let end = (self.scroll_row + self.max_visible_rows).min(rows.len());
        let mut lines = Vec::new();

        if self.scroll_row > 0 {
            lines.push(styled(
                &ansi::fg(self.theme.border),
                &format!("─── ↑ {} more ───", self.scroll_row),
            ));
        }

        for row_idx in self.scroll_row..end {
            lines.push(self.render_content_line(&buffer, row_idx, content_width));
        }

        let remaining = rows.len().saturating_sub(end);
        if remaining > 0 {
            lines.push(styled(
                &ansi::fg(self.theme.border),
                &format!("─── ↓ {remaining} more ───"),
            ));
        }

        pad_lines(&lines, self.padding_x as usize, 0)
    }

    fn insert_normalized(&mut self, normalized: String, preview_width: usize) {
        if should_collapse_paste(&normalized) {
            let collapsed = CollapsedPaste::new(normalized, preview_width, self.cursor);
            shift_paste_offsets_for_insert(&mut self.pastes, self.cursor, collapsed.summary.len());
            self.text.insert_str(self.cursor, &collapsed.summary);
            self.cursor += collapsed.summary.len();
            self.pastes.push(collapsed);
        } else {
            shift_paste_offsets_for_insert(&mut self.pastes, self.cursor, normalized.len());
            self.text.insert_str(self.cursor, &normalized);
            self.cursor += normalized.len();
        }
    }

    fn kill_and_apply(&mut self, deleted: &str, prepend: bool, accumulate: bool, next: String, cursor: usize) {
        self.kill_ring.push(deleted, prepend, accumulate);
        self.text = next;
        self.cursor = cursor;
        if !self.pastes.is_empty() {
            reconcile_paste_offsets(&self.text, &mut self.pastes);
        }
        self.notify_change();
        self.invalidate();
    }

    fn delete_paste_block_at(&mut self, range: std::ops::Range<usize>) -> bool {
        let Some(next) = remove_paste_block_and_adjust(&self.text, range.clone(), &mut self.pastes) else {
            return false;
        };
        self.text = next;
        self.cursor = range.start;
        self.last_yank_len = 0;
        self.notify_change();
        self.invalidate();
        true
    }

    fn delete_scalar_range(&mut self, range: std::ops::Range<usize>, next: String, cursor: usize) {
        adjust_pastes_for_delete(&mut self.pastes, range);
        self.text = next;
        self.cursor = cursor;
        self.notify_change();
        self.invalidate();
    }

    fn handle_action(&mut self, action: EditorAction, data: &str) -> InputResult {
        let now = Instant::now();
        match action {
            EditorAction::CursorUp => {
                let buffer = self.layout(self.last_width);
                self.visual_col_pref = Some(buffer.row_column_for_offset(self.cursor).1);
                let pref = self.visual_col_pref;
                self.cursor = buffer.above_offset(self.cursor, pref);
                self.invalidate();
                InputResult::Consumed
            }
            EditorAction::CursorDown => {
                let buffer = self.layout(self.last_width);
                self.visual_col_pref = Some(buffer.row_column_for_offset(self.cursor).1);
                let pref = self.visual_col_pref;
                self.cursor = buffer.below_offset(self.cursor, pref);
                self.invalidate();
                InputResult::Consumed
            }
            EditorAction::CursorLeft => {
                self.cursor = char_left(&self.text, self.cursor);
                self.visual_col_pref = None;
                self.invalidate();
                InputResult::Consumed
            }
            EditorAction::CursorRight => {
                self.cursor = char_right(&self.text, self.cursor);
                self.visual_col_pref = None;
                self.invalidate();
                InputResult::Consumed
            }
            EditorAction::CursorWordLeft => {
                self.cursor = word_left(&self.text, self.cursor);
                self.invalidate();
                InputResult::Consumed
            }
            EditorAction::CursorWordRight => {
                self.cursor = word_right(&self.text, self.cursor);
                self.invalidate();
                InputResult::Consumed
            }
            EditorAction::CursorLineStart => {
                self.cursor = line_start(&self.text, self.cursor);
                self.invalidate();
                InputResult::Consumed
            }
            EditorAction::CursorLineEnd => {
                self.cursor = line_end(&self.text, self.cursor);
                self.invalidate();
                InputResult::Consumed
            }
            EditorAction::PageUp => {
                self.scroll_row = self.scroll_row.saturating_sub(self.max_visible_rows);
                self.invalidate();
                InputResult::Consumed
            }
            EditorAction::PageDown => {
                self.scroll_row = self.scroll_row.saturating_add(self.max_visible_rows);
                self.invalidate();
                InputResult::Consumed
            }
            EditorAction::DeleteCharBackward => {
                self.push_undo();
                if let Some(range) = paste_block_range(&self.text, self.cursor.saturating_sub(1), &self.pastes) {
                    self.delete_paste_block_at(range);
                } else {
                    let (next, cursor) = delete_char_backward(&self.text, self.cursor);
                    let deleted = self.text[cursor..self.cursor].to_string();
                    self.kill_ring.push(&deleted, true, false);
                    self.delete_scalar_range(cursor..self.cursor, next, cursor);
                }
                InputResult::Consumed
            }
            EditorAction::DeleteCharForward => {
                self.push_undo();
                if let Some(range) = paste_block_range(&self.text, self.cursor, &self.pastes) {
                    self.delete_paste_block_at(range);
                } else {
                    let end = char_right(&self.text, self.cursor);
                    let deleted = self.text[self.cursor..end].to_string();
                    let (next, cursor) = delete_char_forward(&self.text, self.cursor);
                    self.kill_ring.push(&deleted, false, false);
                    self.delete_scalar_range(self.cursor..end, next, cursor);
                }
                InputResult::Consumed
            }
            EditorAction::DeleteWordBackward => {
                self.push_undo();
                let start = word_left(&self.text, self.cursor);
                let deleted = self.text[start..self.cursor].to_string();
                let (next, cursor) = delete_word_backward(&self.text, self.cursor);
                self.kill_and_apply(&deleted, true, false, next, cursor);
                InputResult::Consumed
            }
            EditorAction::DeleteWordForward => {
                self.push_undo();
                let end = word_right(&self.text, self.cursor);
                let deleted = self.text[self.cursor..end].to_string();
                let (next, cursor) = delete_word_forward(&self.text, self.cursor);
                self.kill_and_apply(&deleted, false, false, next, cursor);
                InputResult::Consumed
            }
            EditorAction::DeleteToLineStart => {
                self.push_undo();
                let start = line_start(&self.text, self.cursor);
                let deleted = self.text[start..self.cursor].to_string();
                let (next, cursor) = delete_to_line_start(&self.text, self.cursor);
                self.kill_and_apply(&deleted, true, false, next, cursor);
                InputResult::Consumed
            }
            EditorAction::DeleteToLineEnd => {
                self.push_undo();
                let end = line_end(&self.text, self.cursor);
                let deleted = self.text[self.cursor..end].to_string();
                let (next, cursor) = delete_to_line_end(&self.text, self.cursor);
                self.kill_and_apply(&deleted, false, false, next, cursor);
                InputResult::Consumed
            }
            EditorAction::Yank => {
                let yank = self.kill_ring.peek().map(str::to_string);
                if let Some(text) = yank {
                    self.push_undo();
                    shift_paste_offsets_for_insert(&mut self.pastes, self.cursor, text.len());
                    self.text.insert_str(self.cursor, &text);
                    self.last_yank_len = text.len();
                    self.cursor += text.len();
                    self.notify_change();
                    self.invalidate();
                }
                InputResult::Consumed
            }
            EditorAction::YankPop => {
                if self.kill_ring.len() < 2 {
                    return InputResult::Consumed;
                }
                self.kill_ring.rotate();
                let yank = self.kill_ring.peek().map(str::to_string);
                if let Some(text) = yank {
                    self.push_undo();
                    if self.last_yank_len > 0 {
                        let start = self.cursor.saturating_sub(self.last_yank_len);
                        adjust_pastes_for_delete(&mut self.pastes, start..self.cursor);
                        self.text.replace_range(start..self.cursor, "");
                        self.cursor = start;
                    }
                    shift_paste_offsets_for_insert(&mut self.pastes, self.cursor, text.len());
                    self.text.insert_str(self.cursor, &text);
                    self.last_yank_len = text.len();
                    self.cursor += text.len();
                    self.notify_change();
                    self.invalidate();
                }
                InputResult::Consumed
            }
            EditorAction::Undo => {
                if let Some(snap) = self.undo.pop() {
                    self.restore(snap);
                }
                InputResult::Consumed
            }
            EditorAction::NewLine => {
                self.push_undo();
                shift_paste_offsets_for_insert(&mut self.pastes, self.cursor, 1);
                self.text.insert(self.cursor, '\n');
                self.cursor += 1;
                self.paste_burst.reset();
                self.notify_change();
                self.invalidate();
                InputResult::Consumed
            }
            EditorAction::Submit => {
                if self.disable_submit {
                    return InputResult::Consumed;
                }
                if self.paste_burst.should_insert_newline_instead_of_submit(now) {
                    self.push_undo();
                    self.text.insert(self.cursor, '\n');
                    self.cursor += 1;
                    self.paste_burst.reset();
                    self.notify_change();
                    self.invalidate();
                    return InputResult::Consumed;
                }
                let expanded = self.get_expanded_text();
                if let Some(cb) = &mut self.on_submit {
                    cb(&expanded);
                }
                InputResult::Consumed
            }
            EditorAction::Tab => {
                if self.autocomplete_provider.is_some() {
                    self.open_path_autocomplete();
                    if self.pending_autocomplete.is_some() {
                        return InputResult::Consumed;
                    }
                }
                self.push_undo();
                shift_paste_offsets_for_insert(&mut self.pastes, self.cursor, 2);
                self.text.insert_str(self.cursor, "  ");
                self.cursor += 2;
                self.notify_change();
                self.invalidate();
                InputResult::Consumed
            }
            EditorAction::InsertText => {
                if data == "/" && self.autocomplete_provider.is_some() {
                    let result = self.handle_action(EditorAction::InsertText, data);
                    self.open_slash_autocomplete();
                    return result;
                }
                if data.chars().count() == 1 {
                    self.paste_burst.on_plain_char(now);
                } else if line_count(data) >= PASTE_COLLAPSE_MIN_LINES || data.len() >= PASTE_COLLAPSE_MIN_CHARS {
                    self.paste_burst.extend_window(now);
                }
                self.push_undo();
                self.insert_normalized(normalize_paste_text(data), 40);
                self.notify_change();
                self.invalidate();
                InputResult::Consumed
            }
        }
    }
}

impl LineComponent for Editor {
    fn render(&mut self, width: u16) -> Vec<Line> {
        self.last_width = width;
        let key = (self.text.clone(), self.cursor, width, self.scroll_row);
        if self.cache_key.as_ref() == Some(&key) {
            return self.cache_lines.clone();
        }
        let lines = self.build_lines(width);
        self.cache_key = Some(key);
        self.cache_lines = lines.clone();
        lines
    }

    fn invalidate(&mut self) {
        self.cache_key = None;
        self.cache_lines.clear();
    }

    fn handle_input(&mut self, data: &str) -> InputResult {
        if !self.focused {
            return InputResult::Ignored;
        }
        if let Some(action) = match_editor_action(data) {
            return self.handle_action(action, data);
        }
        if data.len() > 1 && !data.starts_with('\x1b') {
            return self.handle_action(EditorAction::InsertText, data);
        }
        InputResult::Ignored
    }

    fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
        self.invalidate();
    }

    fn is_focused(&self) -> bool {
        self.focused
    }
}

fn slice_at_display_col(text: &str, start_col: usize, end_col: usize) -> String {
    if end_col <= start_col {
        return String::new();
    }
    let mut col = 0usize;
    let mut out = String::new();
    for ch in text.chars() {
        if col >= end_col {
            break;
        }
        if col >= start_col {
            out.push(ch);
        }
        col += char_display_width(ch, col);
    }
    out
}

fn slice_at_display_col_offset(text: &str, col: usize) -> usize {
    let mut width = 0usize;
    for (idx, ch) in text.char_indices() {
        if width >= col {
            return idx;
        }
        width += char_display_width(ch, width);
    }
    text.len()
}

#[cfg(test)]
mod tests {
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
