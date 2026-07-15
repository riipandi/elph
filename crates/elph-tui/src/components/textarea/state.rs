//! Self-contained multiline editor buffer (tui-textarea style).

use iocraft::prelude::*;

use super::layout::layout_cursor_for_viewport;
use crate::paste::apply_paste_at_cursor;
use crate::text_editing::wire_insert_newline;
use crate::text_input_layout::WrappedTextLayout;

/// Live editor buffer — single source of truth for text and cursor.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TextareaState {
    pub text: String,
    pub cursor: usize,
    pub(crate) vertical_col_preference: Option<u16>,
}

impl TextareaState {
    pub fn from_text(text: String) -> Self {
        Self {
            cursor: text.len(),
            text,
            ..Self::default()
        }
    }

    /// Sync from an external [`State`] when the parent mutates the draft.
    pub fn sync_external(&mut self, external: &str) {
        if self.text == external {
            return;
        }
        let was_at_eof = self.cursor == self.text.len();
        let suffix_append = external.len() > self.text.len() && external.starts_with(&self.text);
        self.text = external.to_string();
        self.cursor = if was_at_eof && suffix_append {
            self.text.len()
        } else {
            self.cursor.min(self.text.len())
        };
        self.vertical_col_preference = None;
    }

    /// Cursor for layout/render (maps trailing `\n` to empty continuation row).
    pub fn layout_cursor(&self, _input_width: u16) -> usize {
        layout_cursor_for_viewport(&self.text, self.cursor)
    }

    pub fn insert_char(&mut self, c: char) {
        if c == '\n' || c == '\r' {
            self.insert_newline();
            return;
        }
        let cursor = self.cursor.min(self.text.len());
        self.text.insert(cursor, c);
        self.cursor = cursor + c.len_utf8();
        self.vertical_col_preference = None;
    }

    pub fn insert_newline(&mut self) {
        let (text, cursor) = wire_insert_newline(&self.text, self.cursor);
        self.text = text;
        self.cursor = cursor;
        self.vertical_col_preference = None;
    }

    pub fn delete_char_back(&mut self) {
        let cursor = self.cursor.min(self.text.len());
        if cursor == 0 {
            return;
        }
        let prev = self.text[..cursor].chars().last().map(|c| c.len_utf8()).unwrap_or(0);
        self.text.drain(cursor - prev..cursor);
        self.cursor = cursor - prev;
        self.vertical_col_preference = None;
    }

    pub fn delete_char_forward(&mut self) {
        let cursor = self.cursor.min(self.text.len());
        if cursor >= self.text.len() {
            return;
        }
        let next = self.text[cursor..].chars().next().map(|c| c.len_utf8()).unwrap_or(0);
        self.text.drain(cursor..cursor + next);
        self.vertical_col_preference = None;
    }

    pub fn move_left(&mut self, input_width: u16) {
        self.cursor = WrappedTextLayout::left_of_offset(&self.text, self.cursor);
        self.vertical_col_preference = None;
        let _ = input_width;
    }

    pub fn move_right(&mut self, input_width: u16) {
        self.cursor = WrappedTextLayout::right_of_offset(&self.text, self.cursor);
        self.vertical_col_preference = None;
        let _ = input_width;
    }

    pub fn move_up(&mut self, input_width: u16) {
        let layout = WrappedTextLayout::new_for_overlay_editor(&self.text, input_width);
        if self.vertical_col_preference.is_none() {
            let (_, col) = layout.row_column_for_offset(&self.text, self.cursor);
            self.vertical_col_preference = Some(col);
        }
        self.cursor = layout.above_offset(&self.text, self.cursor, self.vertical_col_preference);
    }

    pub fn move_down(&mut self, input_width: u16) {
        let layout = WrappedTextLayout::new_for_overlay_editor(&self.text, input_width);
        if self.vertical_col_preference.is_none() {
            let (_, col) = layout.row_column_for_offset(&self.text, self.cursor);
            self.vertical_col_preference = Some(col);
        }
        self.cursor = layout.below_offset(&self.text, self.cursor, self.vertical_col_preference);
    }

    pub fn move_home(&mut self, input_width: u16) {
        let layout = WrappedTextLayout::new_for_overlay_editor(&self.text, input_width);
        self.cursor = layout.row_start_offset(&self.text, self.cursor);
        self.vertical_col_preference = None;
    }

    pub fn move_end(&mut self, input_width: u16) {
        let layout = WrappedTextLayout::new_for_overlay_editor(&self.text, input_width);
        self.cursor = layout.row_end_offset(&self.text, self.cursor);
        self.vertical_col_preference = None;
    }

    pub fn input_basic_key(
        &mut self,
        code: KeyCode,
        kind: KeyEventKind,
        modifiers: KeyModifiers,
        input_width: u16,
    ) -> bool {
        if kind == KeyEventKind::Release {
            return false;
        }
        if modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::META) {
            match (modifiers.contains(KeyModifiers::CONTROL), code) {
                (true, KeyCode::Char('a')) => {
                    self.move_home(input_width);
                    return true;
                }
                (true, KeyCode::Char('e')) => {
                    self.move_end(input_width);
                    return true;
                }
                _ => return false,
            }
        }
        match code {
            KeyCode::Char(c) => {
                self.insert_char(c);
                true
            }
            KeyCode::Backspace => {
                self.delete_char_back();
                true
            }
            KeyCode::Delete => {
                self.delete_char_forward();
                true
            }
            KeyCode::Left => {
                self.move_left(input_width);
                true
            }
            KeyCode::Right => {
                self.move_right(input_width);
                true
            }
            KeyCode::Up => {
                self.move_up(input_width);
                true
            }
            KeyCode::Down => {
                self.move_down(input_width);
                true
            }
            KeyCode::Home => {
                self.move_home(input_width);
                true
            }
            KeyCode::End => {
                self.move_end(input_width);
                true
            }
            _ => false,
        }
    }

    pub fn apply_paste(&mut self, data: &str) {
        let cursor = self.cursor.min(self.text.len());
        let (text, cursor) = apply_paste_at_cursor(&self.text, cursor, data);
        self.text = text;
        self.cursor = cursor;
        self.vertical_col_preference = None;
    }

    pub fn clear_after_submit(&mut self) {
        self.text.clear();
        self.cursor = 0;
        self.vertical_col_preference = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_and_delete_char() {
        let mut state = TextareaState::default();
        state.insert_char('h');
        state.insert_char('i');
        assert_eq!(state.text, "hi");
        assert_eq!(state.cursor, 2);
        state.delete_char_back();
        assert_eq!(state.text, "h");
        assert_eq!(state.cursor, 1);
    }

    #[test]
    fn insert_newline_at_eof() {
        let mut state = TextareaState::from_text("hi".into());
        state.cursor = 2;
        state.insert_newline();
        assert_eq!(state.text, "hi\n");
        assert_eq!(state.cursor, 3);
    }

    #[test]
    fn sync_external_suffix_append_advances_cursor_to_eof() {
        let mut state = TextareaState::from_text("hello".into());
        state.cursor = 5;
        state.sync_external("hello world");
        assert_eq!(state.text, "hello world");
        assert_eq!(state.cursor, 11);
    }

    #[test]
    fn sync_external_non_suffix_clamps_cursor() {
        let mut state = TextareaState::from_text("hello".into());
        state.cursor = 3;
        state.sync_external("hi there");
        assert_eq!(state.cursor, 3);
    }
}
