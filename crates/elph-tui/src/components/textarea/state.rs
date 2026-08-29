//! Self-contained multiline editor buffer (tui-textarea style).

use iocraft::prelude::*;
use iocraft::prelude::{KeyCode, KeyModifiers};

use super::layout::layout_cursor_for_viewport;
use crate::paste::apply_paste_at_cursor;
use crate::text_editing::wire_insert_newline;
use crate::text_input_layout::WrappedTextLayout;

/// Live editor buffer — single source of truth for text and cursor.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TextareaState {
    pub text: String,
    pub cursor: usize,
    /// Visual selection anchor (byte index). Selection is `[min(anchor,cursor), max(...))`.
    pub selection_anchor: Option<usize>,
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

    /// Inclusive-exclusive byte range of the active selection, if any.
    pub fn selection_range(&self) -> Option<(usize, usize)> {
        let anchor = self.selection_anchor?;
        let a = anchor.min(self.cursor).min(self.text.len());
        let b = anchor.max(self.cursor).min(self.text.len());
        if a == b { None } else { Some((a, b)) }
    }

    pub fn has_selection(&self) -> bool {
        self.selection_range().is_some()
    }

    pub fn selected_text(&self) -> Option<&str> {
        let (a, b) = self.selection_range()?;
        Some(&self.text[a..b])
    }

    pub fn clear_selection(&mut self) {
        self.selection_anchor = None;
    }

    /// Begin or restart a selection with the anchor at `offset` (cursor becomes the head).
    pub fn begin_selection_at(&mut self, offset: usize) {
        let offset = offset.min(self.text.len());
        self.selection_anchor = Some(offset);
        self.cursor = offset;
        self.vertical_col_preference = None;
    }

    /// Move the selection head (cursor) without moving the anchor.
    pub fn extend_selection_to(&mut self, offset: usize) {
        let offset = offset.min(self.text.len());
        if self.selection_anchor.is_none() {
            self.selection_anchor = Some(self.cursor);
        }
        self.cursor = offset;
    }

    /// Delete the selected range (if any) and place the cursor at the start. Returns true if deleted.
    pub fn delete_selection(&mut self) -> bool {
        let Some((a, b)) = self.selection_range() else {
            return false;
        };
        let (a, b) = expand_image_marker_range(&self.text, a, b);
        self.text.drain(a..b);
        self.cursor = a;
        self.clear_selection();
        self.vertical_col_preference = None;
        true
    }

    /// Whether a focused editor should accept a parent draft update.
    ///
    /// While focused, keystrokes live only in the editor buffer until mirrored to `live_draft`.
    /// A stale parent `draft` must not clobber text the user typed after slash completion.
    pub fn should_sync_focused_external(editor: &str, external: &str) -> bool {
        if external.is_empty() || editor == external {
            return false;
        }
        if editor.starts_with(external) && editor.len() > external.len() {
            return false;
        }
        if external.starts_with(editor) && external.len() > editor.len() {
            if let (Some(ed_query), Some(ext_query)) = (slash_command_query(editor), slash_command_query(external))
                && ed_query == ext_query
            {
                // Same command token — parent only added trailing space/args decoration.
                // Do not restore suffix the user just deleted.
                return false;
            }
            return true;
        }
        if let (Some(ed_query), Some(ext_query)) = (slash_command_query(editor), slash_command_query(external)) {
            return ext_query.starts_with(ed_query) && ext_query.len() > ed_query.len();
        }
        false
    }

    /// Sync from an external [`State`] when the parent mutates the draft.
    pub fn sync_external(&mut self, external: &str) {
        self.sync_external_with_cursor(external, None);
    }

    /// Sync from parent draft, optionally forcing the caret byte index.
    pub fn sync_external_with_cursor(&mut self, external: &str, forced_cursor: Option<usize>) {
        if self.text == external {
            if let Some(pos) = forced_cursor {
                self.cursor = pos.min(self.text.len());
                self.vertical_col_preference = None;
            }
            return;
        }
        let was_at_eof = self.cursor == self.text.len();
        let suffix_append = external.len() > self.text.len() && external.starts_with(&self.text);
        let slash_completion = self.text.starts_with('/') && external.starts_with('/');
        let mention_completion = mention_completion_replaces_query(&self.text, external);
        self.text = external.to_string();
        self.cursor = forced_cursor.unwrap_or_else(|| {
            if (was_at_eof && suffix_append) || slash_completion || (was_at_eof && mention_completion) {
                self.text.len()
            } else {
                self.cursor.min(self.text.len())
            }
        });
        self.cursor = self.cursor.min(self.text.len());
        if let Some(anchor) = self.selection_anchor {
            self.selection_anchor = Some(anchor.min(self.text.len()));
        }
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
        // Replace an active range, then drop any zero-width leftover anchor so Shifted
        // character keys never leave a sticky selection for the next keystroke.
        if !self.delete_selection() {
            self.clear_selection();
        }
        let cursor = self.cursor.min(self.text.len());
        self.text.insert(cursor, c);
        self.cursor = cursor + c.len_utf8();
        self.vertical_col_preference = None;
    }

    pub fn insert_newline(&mut self) {
        self.delete_selection();
        let (text, cursor) = wire_insert_newline(&self.text, self.cursor);
        self.text = text;
        self.cursor = cursor;
        self.vertical_col_preference = None;
    }

    pub fn delete_char_back(&mut self) {
        if self.delete_selection() {
            return;
        }
        let cursor = self.cursor.min(self.text.len());
        if cursor == 0 {
            return;
        }
        if let Some((start, end)) = image_marker_range_at(&self.text, cursor.saturating_sub(1)) {
            self.text.drain(start..end);
            self.cursor = start;
        } else {
            let prev = self.text[..cursor].chars().last().map(|c| c.len_utf8()).unwrap_or(0);
            self.text.drain(cursor - prev..cursor);
            self.cursor = cursor - prev;
        }
        self.vertical_col_preference = None;
    }

    pub fn delete_char_forward(&mut self) {
        if self.delete_selection() {
            return;
        }
        let cursor = self.cursor.min(self.text.len());
        if cursor >= self.text.len() {
            return;
        }
        if let Some((start, end)) = image_marker_range_at(&self.text, cursor) {
            self.text.drain(start..end);
        } else {
            let next = self.text[cursor..].chars().next().map(|c| c.len_utf8()).unwrap_or(0);
            self.text.drain(cursor..cursor + next);
        }
        self.vertical_col_preference = None;
    }

    pub fn move_left(&mut self, input_width: u16) {
        let _ = input_width;
        self.clear_selection();
        self.cursor = image_marker_end_or_left(&self.text, self.cursor)
            .unwrap_or_else(|| WrappedTextLayout::left_of_offset(&self.text, self.cursor));
        self.vertical_col_preference = None;
    }

    pub fn move_right(&mut self, input_width: u16) {
        let _ = input_width;
        self.clear_selection();
        self.cursor = image_marker_start_or_right(&self.text, self.cursor)
            .unwrap_or_else(|| WrappedTextLayout::right_of_offset(&self.text, self.cursor));
        self.vertical_col_preference = None;
    }

    pub fn move_up(&mut self, input_width: u16) {
        self.clear_selection();
        let layout = WrappedTextLayout::new_for_overlay_editor(&self.text, input_width);
        if self.vertical_col_preference.is_none() {
            let (_, col) = layout.row_column_for_offset(&self.text, self.cursor);
            self.vertical_col_preference = Some(col);
        }
        self.cursor = layout.above_offset(&self.text, self.cursor, self.vertical_col_preference);
    }

    pub fn move_down(&mut self, input_width: u16) {
        self.clear_selection();
        let layout = WrappedTextLayout::new_for_overlay_editor(&self.text, input_width);
        if self.vertical_col_preference.is_none() {
            let (_, col) = layout.row_column_for_offset(&self.text, self.cursor);
            self.vertical_col_preference = Some(col);
        }
        self.cursor = layout.below_offset(&self.text, self.cursor, self.vertical_col_preference);
    }

    pub fn move_home(&mut self, input_width: u16) {
        self.clear_selection();
        let layout = WrappedTextLayout::new_for_overlay_editor(&self.text, input_width);
        self.cursor = layout.row_start_offset(&self.text, self.cursor);
        self.vertical_col_preference = None;
    }

    pub fn move_end(&mut self, input_width: u16) {
        self.clear_selection();
        let layout = WrappedTextLayout::new_for_overlay_editor(&self.text, input_width);
        self.cursor = layout.row_end_offset(&self.text, self.cursor);
        self.vertical_col_preference = None;
    }

    /// Arrow / Home / End with Shift extends the selection.
    ///
    /// Only navigation keys may start a selection. Shifted *characters* (e.g. `@` via
    /// Shift+2) must not set [`Self::selection_anchor`] — otherwise the next keystroke
    /// replaces the just-typed char via [`Self::delete_selection`].
    pub fn move_with_shift(&mut self, code: KeyCode, input_width: u16) -> bool {
        match code {
            KeyCode::Left | KeyCode::Right | KeyCode::Up | KeyCode::Down | KeyCode::Home | KeyCode::End => {}
            // Char / Tab / Enter with Shift held are typing, not selection.
            _ => return false,
        }

        let layout = WrappedTextLayout::new_for_overlay_editor(&self.text, input_width);
        if self.selection_anchor.is_none() {
            self.selection_anchor = Some(self.cursor);
        }
        match code {
            KeyCode::Left => {
                self.cursor = WrappedTextLayout::left_of_offset(&self.text, self.cursor);
                self.vertical_col_preference = None;
                true
            }
            KeyCode::Right => {
                self.cursor = WrappedTextLayout::right_of_offset(&self.text, self.cursor);
                self.vertical_col_preference = None;
                true
            }
            KeyCode::Up => {
                if self.vertical_col_preference.is_none() {
                    let (_, col) = layout.row_column_for_offset(&self.text, self.cursor);
                    self.vertical_col_preference = Some(col);
                }
                self.cursor = layout.above_offset(&self.text, self.cursor, self.vertical_col_preference);
                true
            }
            KeyCode::Down => {
                if self.vertical_col_preference.is_none() {
                    let (_, col) = layout.row_column_for_offset(&self.text, self.cursor);
                    self.vertical_col_preference = Some(col);
                }
                self.cursor = layout.below_offset(&self.text, self.cursor, self.vertical_col_preference);
                true
            }
            KeyCode::Home => {
                self.cursor = layout.row_start_offset(&self.text, self.cursor);
                self.vertical_col_preference = None;
                true
            }
            KeyCode::End => {
                self.cursor = layout.row_end_offset(&self.text, self.cursor);
                self.vertical_col_preference = None;
                true
            }
            _ => false,
        }
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
        self.delete_selection();
        let cursor = self.cursor.min(self.text.len());
        let (text, cursor) = apply_paste_at_cursor(&self.text, cursor, data);
        self.text = text;
        self.cursor = cursor;
        self.vertical_col_preference = None;
    }

    /// Insert an image marker as one cursor/deletion unit.
    pub fn insert_image_marker(&mut self, id: usize) {
        self.delete_selection();
        let marker = format!("[Image #{id}]");
        let cursor = self.cursor.min(self.text.len());
        self.text.insert_str(cursor, &marker);
        self.cursor = cursor + marker.len();
        self.vertical_col_preference = None;
    }

    pub fn clear_after_submit(&mut self) {
        self.text.clear();
        self.cursor = 0;
        self.clear_selection();
        self.vertical_col_preference = None;
    }
}

fn image_marker_range_at(text: &str, offset: usize) -> Option<(usize, usize)> {
    let mut search_from = 0;
    while let Some(relative) = text[search_from..].find("[Image #") {
        let start = search_from + relative;
        let digits_start = start + "[Image #".len();
        let end = text[digits_start..].find(']')? + digits_start + 1;
        if digits_start < end - 1
            && text[digits_start..end - 1].chars().all(|ch| ch.is_ascii_digit())
            && offset >= start
            && offset < end
        {
            return Some((start, end));
        }
        search_from = end;
    }
    None
}

/// Return the marker range and image id touching a caret offset.
pub fn image_marker_id_at_cursor(text: &str, cursor: usize) -> Option<(usize, usize, usize)> {
    let marker =
        image_marker_range_at(text, cursor).or_else(|| image_marker_range_at(text, cursor.saturating_sub(1)))?;
    let id_start = marker.0 + "[Image #".len();
    let id = text[id_start..marker.1 - 1].parse().ok()?;
    Some((marker.0, marker.1, id))
}

fn expand_image_marker_range(text: &str, mut start: usize, mut end: usize) -> (usize, usize) {
    let mut cursor = 0;
    while let Some(relative) = text[cursor..].find("[Image #") {
        let marker_start = cursor + relative;
        let digits_start = marker_start + "[Image #".len();
        let Some(relative_end) = text[digits_start..].find(']') else {
            break;
        };
        let marker_end = digits_start + relative_end + 1;
        if digits_start < marker_end - 1
            && text[digits_start..marker_end - 1].chars().all(|ch| ch.is_ascii_digit())
            && start < marker_end
            && end > marker_start
        {
            start = start.min(marker_start);
            end = end.max(marker_end);
        }
        cursor = marker_end;
    }
    (start, end)
}

fn image_marker_end_or_left(text: &str, cursor: usize) -> Option<usize> {
    let marker = image_marker_range_at(text, cursor.saturating_sub(1))?;
    (marker.1 == cursor).then_some(marker.0)
}

fn image_marker_start_or_right(text: &str, cursor: usize) -> Option<usize> {
    let marker = image_marker_range_at(text, cursor)?;
    (marker.0 == cursor).then_some(marker.1)
}

#[cfg(test)]
mod image_marker_tests {
    use super::{TextareaState, image_marker_id_at_cursor};

    #[test]
    fn image_marker_is_inserted_and_deleted_atomically() {
        let mut state = TextareaState::from_text("before after".into());
        state.cursor = "before ".len();
        state.insert_image_marker(1);
        assert_eq!(state.text, "before [Image #1]after");

        state.delete_char_back();
        assert_eq!(state.text, "before after");
        assert_eq!(state.cursor, "before ".len());
    }

    #[test]
    fn cursor_skips_image_marker() {
        let mut state = TextareaState::from_text("[Image #7]x".into());
        state.cursor = "[Image #7]".len();
        state.move_left(80);
        assert_eq!(state.cursor, 0);
        state.move_right(80);
        assert_eq!(state.cursor, "[Image #7]".len());
    }

    #[test]
    fn image_marker_preview_range_includes_caret_after_marker() {
        let mut state = TextareaState::from_text("x[Image #7]y".into());
        state.cursor = 10;
        assert_eq!(image_marker_id_at_cursor(&state.text, state.cursor), Some((1, 11, 7)));
        state.cursor = 11;
        assert_eq!(image_marker_id_at_cursor(&state.text, state.cursor), Some((1, 11, 7)));
        state.cursor = 12;
        assert_eq!(image_marker_id_at_cursor(&state.text, state.cursor), None);
    }
}

/// Build per-row text segments for selection highlighting (buffer offsets only — no borders).
///
/// Each row is a list of `(text, selected)` segments for mixed-style rendering.
pub fn selection_display_rows(
    text: &str,
    layout: &WrappedTextLayout,
    selection: Option<(usize, usize)>,
    scroll_row: u16,
    viewport_rows: u16,
) -> Vec<Vec<(String, bool)>> {
    let lines = layout.wrapped_line_strings(text);
    let start = scroll_row as usize;
    let end = (start + viewport_rows as usize).min(lines.len().max(1));
    let mut out = Vec::with_capacity(end.saturating_sub(start).max(1));
    if lines.is_empty() {
        out.push(vec![(String::new(), false)]);
        return out;
    }
    for (i, line) in lines.iter().enumerate().take(end).skip(start) {
        let row = &layout.rows.get(i);
        let Some(row) = row else {
            out.push(vec![(line.clone(), false)]);
            continue;
        };
        let row_start = row.offset;
        let row_end = row.offset + row.len;
        let Some((sel_a, sel_b)) = selection else {
            out.push(vec![(line.clone(), false)]);
            continue;
        };
        let a = sel_a.max(row_start).min(row_end);
        let b = sel_b.max(row_start).min(row_end);
        if a >= b {
            out.push(vec![(line.clone(), false)]);
            continue;
        }
        let mut segs = Vec::new();
        if a > row_start {
            segs.push((text[row_start..a].to_string(), false));
        }
        segs.push((text[a..b].to_string(), true));
        if b < row_end {
            segs.push((text[b..row_end].to_string(), false));
        }
        if segs.is_empty() {
            segs.push((String::new(), false));
        }
        out.push(segs);
    }
    if out.is_empty() {
        out.push(vec![(String::new(), false)]);
    }
    out
}

fn mention_completion_replaces_query(before: &str, after: &str) -> bool {
    let Some(ed_at) = before.rfind('@') else {
        return false;
    };
    let Some(ext_at) = after.rfind('@') else {
        return false;
    };
    ed_at == ext_at && before[..ed_at] == after[..ext_at] && after.len() > before.len()
}

fn slash_command_query(draft: &str) -> Option<&str> {
    let trimmed = draft.trim_start();
    if !trimmed.starts_with('/') {
        return None;
    }
    let body = trimmed.trim_start_matches('/').trim_start();
    let (name, _) = body.split_once(' ').map_or((body, ""), |(n, _)| (n, ""));
    if name.is_empty() { None } else { Some(name) }
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

    #[test]
    fn sync_external_slash_completion_lands_cursor_at_eof() {
        let mut state = TextareaState::from_text("/goal".into());
        state.cursor = 2;
        state.sync_external("/compact ");
        assert_eq!(state.text, "/compact ");
        assert_eq!(state.cursor, 9);
    }

    #[test]
    fn sync_external_mention_completion_lands_cursor_at_eof() {
        let mut state = TextareaState::from_text("fix @main".into());
        state.cursor = state.text.len();
        state.sync_external("fix @src/main.rs ");
        assert_eq!(state.text, "fix @src/main.rs ");
        assert_eq!(state.cursor, state.text.len());
    }

    #[test]
    fn sync_external_with_cursor_overrides_mention_heuristic() {
        let mut state = TextareaState::from_text("fix @main".into());
        state.cursor = 4;
        state.sync_external_with_cursor("fix @src/main.rs ", Some(17));
        assert_eq!(state.cursor, 17);
    }

    #[test]
    fn focused_sync_accepts_slash_completion_prefix() {
        assert!(TextareaState::should_sync_focused_external("/go", "/goal "));
        assert!(TextareaState::should_sync_focused_external("/c", "/compact "));
    }

    #[test]
    fn focused_sync_rejects_stale_parent_after_typing() {
        assert!(!TextareaState::should_sync_focused_external("/goal args", "/goal "));
        assert!(!TextareaState::should_sync_focused_external("/goal x", "/goal "));
    }

    #[test]
    fn focused_sync_rejects_stale_parent_after_deletion() {
        assert!(!TextareaState::should_sync_focused_external("/goal", "/goal "));
        assert!(!TextareaState::should_sync_focused_external("/goal pause", "/goal "));
    }

    #[test]
    fn shift_char_does_not_start_selection_or_replace_previous() {
        // Regression: Shift+@ (mention) then `k` used to replace `@` because
        // `move_with_shift` set selection_anchor before rejecting non-nav keys.
        let mut state = TextareaState::default();
        assert!(!state.move_with_shift(KeyCode::Char('@'), 40));
        assert!(state.selection_anchor.is_none());
        state.insert_char('@');
        assert_eq!(state.text, "@");
        assert!(state.selection_anchor.is_none());

        assert!(!state.move_with_shift(KeyCode::Char('k'), 40));
        state.insert_char('k');
        assert_eq!(state.text, "@k");
        assert_eq!(state.cursor, 2);
        assert!(!state.has_selection());
    }

    #[test]
    fn shift_arrow_still_starts_selection() {
        let mut state = TextareaState::from_text("hello".into());
        state.cursor = 5;
        assert!(state.move_with_shift(KeyCode::Left, 40));
        assert_eq!(state.selection_anchor, Some(5));
        assert_eq!(state.cursor, 4);
        assert!(state.has_selection());
    }
}
