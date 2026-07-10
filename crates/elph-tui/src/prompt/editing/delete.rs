use slt::TextareaState;

use super::grapheme::{byte_index_for_grapheme, grapheme_count, next_word_col, prev_word_col};
use super::movement::{current_line, normalize_cursor};

pub(super) fn delete_range(state: &mut TextareaState, start_col: usize, end_col: usize) {
    normalize_cursor(state);
    let start_col = start_col.min(end_col);
    let end_col = end_col.max(start_col);
    if start_col == end_col {
        return;
    }
    let line = &mut state.lines[state.cursor_row];
    let start = byte_index_for_grapheme(line, start_col);
    let end = byte_index_for_grapheme(line, end_col);
    line.replace_range(start..end, "");
    state.cursor_col = start_col;
}

pub(super) fn delete_word_backward(state: &mut TextareaState) {
    normalize_cursor(state);
    if state.cursor_col > 0 {
        let target = prev_word_col(current_line(state), state.cursor_col);
        delete_range(state, target, state.cursor_col);
    } else if state.cursor_row > 0 {
        let current = state.lines.remove(state.cursor_row);
        state.cursor_row -= 1;
        state.cursor_col = grapheme_count(&state.lines[state.cursor_row]);
        state.lines[state.cursor_row].push_str(&current);
    }
}

pub(super) fn delete_word_forward(state: &mut TextareaState) {
    normalize_cursor(state);
    let line_len = grapheme_count(current_line(state));
    if state.cursor_col < line_len {
        let target = next_word_col(current_line(state), state.cursor_col);
        delete_range(state, state.cursor_col, target);
    } else if state.cursor_row + 1 < state.lines.len() {
        let next = state.lines.remove(state.cursor_row + 1);
        state.lines[state.cursor_row].push_str(&next);
    }
}

pub(super) fn delete_to_line_start(state: &mut TextareaState) {
    normalize_cursor(state);
    if state.cursor_col > 0 {
        delete_range(state, 0, state.cursor_col);
    }
}

pub(super) fn delete_to_line_end(state: &mut TextareaState) {
    normalize_cursor(state);
    let line_len = grapheme_count(current_line(state));
    if state.cursor_col < line_len {
        delete_range(state, state.cursor_col, line_len);
    }
}

pub(super) fn insert_newline(state: &mut TextareaState) {
    normalize_cursor(state);
    let split_index = byte_index_for_grapheme(&state.lines[state.cursor_row], state.cursor_col);
    let remainder = state.lines[state.cursor_row].split_off(split_index);
    state.cursor_row += 1;
    state.lines.insert(state.cursor_row, remainder);
    state.cursor_col = 0;
}
