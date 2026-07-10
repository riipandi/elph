use super::movement::{
    char_left, char_right, line_end, line_start, should_delete_by_char_backward, should_delete_by_char_forward,
    word_left, word_right,
};

/// Delete from start of current line through `cursor`.
pub fn delete_to_line_start(text: &str, cursor: usize) -> (String, usize) {
    let cursor = cursor.min(text.len());
    let start = line_start(text, cursor);
    if start == cursor {
        if cursor == 0 {
            return (text.to_string(), 0);
        }
        // Empty / whitespace-only line: delete backward (typically merges lines).
        return delete_char_backward(text, cursor);
    }
    let mut next = text.to_string();
    next.drain(start..cursor);
    (next, start)
}

/// Delete from `cursor` through end of current line.
pub fn delete_to_line_end(text: &str, cursor: usize) -> (String, usize) {
    let cursor = cursor.min(text.len());
    let end = line_end(text, cursor);
    if end == cursor {
        if cursor >= text.len() {
            return (text.to_string(), cursor);
        }
        return delete_char_forward(text, cursor);
    }
    let mut next = text.to_string();
    next.drain(cursor..end);
    (next, cursor)
}

/// Delete the character before `cursor`.
pub fn delete_char_backward(text: &str, cursor: usize) -> (String, usize) {
    let cursor = cursor.min(text.len());
    if cursor == 0 {
        return (text.to_string(), 0);
    }
    let start = char_left(text, cursor);
    let mut next = text.to_string();
    next.drain(start..cursor);
    (next, start)
}

/// Delete the character at `cursor`.
pub fn delete_char_forward(text: &str, cursor: usize) -> (String, usize) {
    let cursor = cursor.min(text.len());
    if cursor >= text.len() {
        return (text.to_string(), cursor);
    }
    let end = char_right(text, cursor);
    let mut next = text.to_string();
    next.drain(cursor..end);
    (next, cursor)
}

/// Delete the word before `cursor` (macOS Option+Backspace / Ctrl+W).
pub fn delete_word_backward(text: &str, cursor: usize) -> (String, usize) {
    let cursor = cursor.min(text.len());
    if cursor == 0 {
        return (text.to_string(), 0);
    }
    if should_delete_by_char_backward(text, cursor) {
        return delete_char_backward(text, cursor);
    }
    let start = word_left(text, cursor);
    if start == cursor {
        return delete_char_backward(text, cursor);
    }
    let mut next = text.to_string();
    next.drain(start..cursor);
    (next, start)
}

/// Delete the word after `cursor` (macOS Option+Delete).
pub fn delete_word_forward(text: &str, cursor: usize) -> (String, usize) {
    let cursor = cursor.min(text.len());
    if cursor >= text.len() {
        return (text.to_string(), cursor);
    }
    if should_delete_by_char_forward(text, cursor) {
        return delete_char_forward(text, cursor);
    }
    let end = word_right(text, cursor);
    if end == cursor {
        return delete_char_forward(text, cursor);
    }
    let mut next = text.to_string();
    next.drain(cursor..end);
    (next, cursor)
}
