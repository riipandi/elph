//! Text mutation actions (delete, newline, word motion).

use super::line::{line_end_offset, line_start_offset, next_word_offset, prev_word_offset};

/// Editing action triggered by platform-style keyboard shortcuts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextEditAction {
    WordLeft,
    WordRight,
    DeleteWordBackward,
    DeleteWordForward,
    DeleteToLineStart,
    DeleteToLineEnd,
    InsertNewline,
}

fn line_is_blank(line: &str) -> bool {
    line.chars().all(|c| c.is_whitespace())
}

fn delete_preceding_newline_if_at_line_start(text: &str, cursor: usize) -> Option<(String, usize)> {
    let cursor = cursor.min(text.len());
    let start = line_start_offset(text, cursor);
    if start == cursor && start > 0 && text.as_bytes().get(start - 1) == Some(&b'\n') {
        let mut out = text.to_string();
        out.remove(start - 1);
        Some((out, start - 1))
    } else {
        None
    }
}

fn delete_blank_lines_backward(text: &str, cursor: usize) -> Option<(String, usize)> {
    let cursor = cursor.min(text.len());
    let line_start = line_start_offset(text, cursor);
    if line_start != cursor || line_start == 0 {
        return None;
    }

    let line_end = line_end_offset(text, cursor);
    let current_line = &text[line_start..line_end];
    if !line_is_blank(current_line) {
        return delete_preceding_newline_if_at_line_start(text, cursor);
    }

    let mut delete_from = line_start;
    let mut scan_line_start = line_start;

    loop {
        if scan_line_start == 0 {
            break;
        }
        let prev_newline = scan_line_start - 1;
        if text.as_bytes().get(prev_newline) != Some(&b'\n') {
            break;
        }
        let prev_line_start = line_start_offset(text, prev_newline);
        let prev_line = &text[prev_line_start..prev_newline];
        if line_is_blank(prev_line) {
            delete_from = prev_line_start;
            scan_line_start = prev_line_start;
        } else {
            delete_from = prev_newline;
            break;
        }
    }

    if delete_from == line_start {
        return delete_preceding_newline_if_at_line_start(text, cursor);
    }

    let mut out = text.to_string();
    out.drain(delete_from..line_start);
    Some((out, delete_from))
}

pub fn delete_word_backward(text: &str, cursor: usize) -> (String, usize) {
    let start = prev_word_offset(text, cursor);
    if start == cursor {
        return delete_preceding_newline_if_at_line_start(text, cursor).unwrap_or_else(|| (text.to_string(), cursor));
    }
    let mut out = text.to_string();
    out.drain(start..cursor);
    (out, start)
}

pub fn delete_word_forward(text: &str, cursor: usize) -> (String, usize) {
    let end = next_word_offset(text, cursor);
    if end == cursor {
        return (text.to_string(), cursor);
    }
    let mut out = text.to_string();
    out.drain(cursor..end);
    (out, cursor)
}

pub fn delete_to_line_start(text: &str, cursor: usize) -> (String, usize) {
    let cursor = cursor.min(text.len());
    let start = line_start_offset(text, cursor);
    if start == cursor {
        return delete_blank_lines_backward(text, cursor)
            .or_else(|| delete_preceding_newline_if_at_line_start(text, cursor))
            .unwrap_or_else(|| (text.to_string(), cursor));
    }
    let mut out = text.to_string();
    out.drain(start..cursor);
    (out, start)
}

pub fn delete_to_line_end(text: &str, cursor: usize) -> (String, usize) {
    let end = line_end_offset(text, cursor);
    if end == cursor {
        return (text.to_string(), cursor);
    }
    let mut out = text.to_string();
    out.drain(cursor..end);
    (out, cursor)
}

pub fn insert_newline_at_cursor(text: &str, cursor: usize) -> (String, usize) {
    let cursor = cursor.min(text.len());
    let mut out = text.to_string();
    out.insert(cursor, '\n');
    (out, cursor + '\n'.len_utf8())
}

/// Newline at EOF places the cursor on the empty continuation row (`text.len()`).
pub fn wire_insert_newline(text: &str, cursor: usize) -> (String, usize) {
    let cursor = cursor.min(text.len());
    let (new_text, mut new_cursor) = insert_newline_at_cursor(text, cursor);
    if new_text.ends_with('\n') && cursor >= text.len() {
        new_cursor = new_text.len();
    }
    (new_text, new_cursor)
}

/// Apply an editing action at `cursor` (byte offset).
pub fn apply_action(action: TextEditAction, text: &str, cursor: usize) -> (String, usize) {
    match action {
        TextEditAction::WordLeft => (text.to_string(), prev_word_offset(text, cursor)),
        TextEditAction::WordRight => (text.to_string(), next_word_offset(text, cursor)),
        TextEditAction::DeleteWordBackward => delete_word_backward(text, cursor),
        TextEditAction::DeleteWordForward => delete_word_forward(text, cursor),
        TextEditAction::DeleteToLineStart => delete_to_line_start(text, cursor),
        TextEditAction::DeleteToLineEnd => delete_to_line_end(text, cursor),
        TextEditAction::InsertNewline => wire_insert_newline(text, cursor),
    }
}
