use std::time::Instant;

use crate::diff::component::InputResult;
use crate::diff::keybindings::EditorAction;
use crate::diff::paste::{
    PASTE_COLLAPSE_MIN_CHARS, PASTE_COLLAPSE_MIN_LINES, adjust_pastes_for_delete, line_count, normalize_paste_text,
    paste_block_range, shift_paste_offsets_for_insert,
};
use crate::diff::text_edit::{
    char_left, char_right, delete_char_backward, delete_char_forward, delete_to_line_end, delete_to_line_start,
    delete_word_backward, delete_word_forward, line_end, line_start, word_left, word_right,
};

use super::Editor;

impl Editor {
    pub(super) fn handle_action(&mut self, action: EditorAction, data: &str) -> InputResult {
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
