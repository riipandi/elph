use slt::TextareaState;

use super::grapheme::{grapheme_count, next_word_col, prev_word_col};

pub(super) fn current_line(state: &TextareaState) -> &str {
    &state.lines[state.cursor_row.min(state.lines.len().saturating_sub(1))]
}

pub(super) fn normalize_cursor(state: &mut TextareaState) {
    if state.lines.is_empty() {
        state.lines.push(String::new());
    }
    state.cursor_row = state.cursor_row.min(state.lines.len().saturating_sub(1));
    state.cursor_col = state.cursor_col.min(grapheme_count(&state.lines[state.cursor_row]));
}

pub(super) fn move_char_left(state: &mut TextareaState) {
    normalize_cursor(state);
    if state.cursor_col > 0 {
        state.cursor_col -= 1;
    } else if state.cursor_row > 0 {
        state.cursor_row -= 1;
        state.cursor_col = grapheme_count(current_line(state));
    }
}

pub(super) fn move_char_right(state: &mut TextareaState) {
    normalize_cursor(state);
    let line_len = grapheme_count(current_line(state));
    if state.cursor_col < line_len {
        state.cursor_col += 1;
    } else if state.cursor_row + 1 < state.lines.len() {
        state.cursor_row += 1;
        state.cursor_col = 0;
    }
}

pub(super) fn move_line_up(state: &mut TextareaState) {
    normalize_cursor(state);
    if state.cursor_row > 0 {
        state.cursor_row -= 1;
        state.cursor_col = state.cursor_col.min(grapheme_count(&state.lines[state.cursor_row]));
    }
}

pub(super) fn move_line_down(state: &mut TextareaState) {
    normalize_cursor(state);
    if state.cursor_row + 1 < state.lines.len() {
        state.cursor_row += 1;
        state.cursor_col = state.cursor_col.min(grapheme_count(&state.lines[state.cursor_row]));
    }
}

pub(super) fn move_word_left(state: &mut TextareaState) {
    normalize_cursor(state);
    if state.cursor_col > 0 {
        state.cursor_col = prev_word_col(current_line(state), state.cursor_col);
    } else if state.cursor_row > 0 {
        state.cursor_row -= 1;
        state.cursor_col = grapheme_count(current_line(state));
    }
}

pub(super) fn move_word_right(state: &mut TextareaState) {
    normalize_cursor(state);
    let line_len = grapheme_count(current_line(state));
    if state.cursor_col < line_len {
        state.cursor_col = next_word_col(current_line(state), state.cursor_col);
    } else if state.cursor_row + 1 < state.lines.len() {
        state.cursor_row += 1;
        state.cursor_col = 0;
    }
}

pub(super) fn move_line_start(state: &mut TextareaState) {
    normalize_cursor(state);
    state.cursor_col = 0;
}

pub(super) fn move_line_end(state: &mut TextareaState) {
    normalize_cursor(state);
    state.cursor_col = grapheme_count(current_line(state));
}
