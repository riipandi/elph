/// Byte offset at the start of the line containing `cursor`.
pub fn line_start(text: &str, cursor: usize) -> usize {
    let cursor = cursor.min(text.len());
    text[..cursor].rfind('\n').map_or(0, |idx| idx + 1)
}

/// Byte offset at the end of the line containing `cursor` (before the newline).
pub fn line_end(text: &str, cursor: usize) -> usize {
    let cursor = cursor.min(text.len());
    text[cursor..].find('\n').map_or(text.len(), |idx| cursor + idx)
}

/// Move one character left from `cursor`.
pub fn char_left(text: &str, cursor: usize) -> usize {
    let cursor = cursor.min(text.len());
    if cursor == 0 {
        return 0;
    }
    prev_char_index(text, cursor)
}

/// Move one character right from `cursor`.
pub fn char_right(text: &str, cursor: usize) -> usize {
    let cursor = cursor.min(text.len());
    if cursor >= text.len() {
        return text.len();
    }
    let ch = text[cursor..].chars().next().unwrap();
    cursor + ch.len_utf8()
}

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

/// Move to the start of the previous word (macOS Option+Left).
pub fn word_left(text: &str, cursor: usize) -> usize {
    let cursor = cursor.min(text.len());
    if cursor == 0 {
        return 0;
    }

    if should_delete_by_char_backward(text, cursor) {
        return prev_char_index(text, cursor);
    }

    let mut i = cursor;
    while i > 0 {
        let ch = text[..i].chars().last().unwrap();
        if is_word_char(ch) {
            break;
        }
        i = prev_char_index(text, i);
    }
    while i > 0 {
        let ch = text[..i].chars().last().unwrap();
        if !is_word_char(ch) {
            break;
        }
        i = prev_char_index(text, i);
    }

    if i == cursor { prev_char_index(text, cursor) } else { i }
}

/// Move to the start of the next word (macOS Option+Right).
pub fn word_right(text: &str, cursor: usize) -> usize {
    let cursor = cursor.min(text.len());
    if cursor >= text.len() {
        return text.len();
    }

    if should_delete_by_char_forward(text, cursor) {
        return char_right(text, cursor);
    }

    let mut i = cursor;
    if is_word_char(text[i..].chars().next().unwrap()) {
        while i < text.len() {
            let ch = text[i..].chars().next().unwrap();
            if !is_word_char(ch) {
                break;
            }
            i += ch.len_utf8();
        }
    }
    while i < text.len() {
        let ch = text[i..].chars().next().unwrap();
        if is_word_char(ch) {
            break;
        }
        i += ch.len_utf8();
    }

    if i == cursor { char_right(text, cursor) } else { i }
}

/// True when the cursor sits on a blank line or a whitespace-only span (no word chars).
fn should_delete_by_char_backward(text: &str, cursor: usize) -> bool {
    let begin = line_start(text, cursor);
    begin == cursor || is_whitespace_only(text, begin, cursor)
}

/// True when the cursor sits before a blank line tail or whitespace-only span.
fn should_delete_by_char_forward(text: &str, cursor: usize) -> bool {
    let end = line_end(text, cursor);
    end == cursor || is_whitespace_only(text, cursor, end)
}

fn is_whitespace_only(text: &str, start: usize, end: usize) -> bool {
    start < end && text[start..end].chars().all(|c| c.is_whitespace())
}

fn prev_char_index(text: &str, index: usize) -> usize {
    text[..index].char_indices().last().map_or(0, |(i, _)| i)
}

fn is_word_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_start_and_end() {
        let text = "hello\nworld";
        assert_eq!(line_start(text, 7), 6);
        assert_eq!(line_end(text, 7), 11);
    }

    #[test]
    fn deletes_to_line_start() {
        let (next, cursor) = delete_to_line_start("hello world", 6);
        assert_eq!(next, "world");
        assert_eq!(cursor, 0);
    }

    #[test]
    fn deletes_to_line_end() {
        let (next, cursor) = delete_to_line_end("hello world", 6);
        assert_eq!(next, "hello ");
        assert_eq!(cursor, 6);
    }

    #[test]
    fn delete_word_backward_removes_previous_word() {
        let (next, cursor) = delete_word_backward("hello world", 11);
        assert_eq!(next, "hello ");
        assert_eq!(cursor, 6);
    }

    #[test]
    fn delete_word_forward_removes_next_word() {
        let (next, cursor) = delete_word_forward("hello world", 0);
        assert_eq!(next, "world");
        assert_eq!(cursor, 0);
    }

    #[test]
    fn char_navigation_moves_by_scalar() {
        assert_eq!(char_left("héllo", 5), 4);
        assert_eq!(char_right("héllo", 3), 4);
    }

    #[test]
    fn word_left_skips_to_previous_word() {
        assert_eq!(word_left("hello world", 11), 6);
        assert_eq!(word_left("hello world", 6), 0);
    }

    #[test]
    fn word_right_skips_to_next_word() {
        assert_eq!(word_right("hello world", 0), 6);
        assert_eq!(word_right("hello world", 6), 11);
    }

    #[test]
    fn delete_to_line_start_on_empty_line_removes_newline() {
        let (next, cursor) = delete_to_line_start("line1\n\nline3", 6);
        assert_eq!(next, "line1\nline3");
        assert_eq!(cursor, 5);
    }

    #[test]
    fn delete_to_line_end_on_empty_line_removes_newline() {
        let (next, cursor) = delete_to_line_end("line1\n\n", 6);
        assert_eq!(next, "line1\n");
        assert_eq!(cursor, 6);
    }

    #[test]
    fn delete_word_backward_on_blank_line_removes_newline() {
        let (next, cursor) = delete_word_backward("line1\n\nline3", 6);
        assert_eq!(next, "line1\nline3");
        assert_eq!(cursor, 5);
    }

    #[test]
    fn delete_word_forward_on_blank_line_removes_newline() {
        let (next, cursor) = delete_word_forward("line1\n\nline3", 6);
        assert_eq!(next, "line1\nline3");
        assert_eq!(cursor, 6);
    }

    #[test]
    fn word_left_on_blank_line_moves_one_char() {
        assert_eq!(word_left("hello\n\n", 6), 5);
    }

    #[test]
    fn word_right_on_blank_line_moves_one_char() {
        assert_eq!(word_right("hello\n\nline", 6), 7);
    }

    #[test]
    fn delete_word_backward_on_whitespace_only_span() {
        let (next, cursor) = delete_word_backward("line1\n   \nline3", 8);
        assert_eq!(next, "line1\n  \nline3");
        assert_eq!(cursor, 7);
    }

    #[test]
    fn delete_to_line_start_on_whitespace_only_line() {
        let (next, cursor) = delete_to_line_start("line1\n   \nline3", 9);
        assert_eq!(next, "line1\n\nline3");
        assert_eq!(cursor, 6);
    }
}
