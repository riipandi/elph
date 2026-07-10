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
pub(super) fn should_delete_by_char_backward(text: &str, cursor: usize) -> bool {
    let begin = line_start(text, cursor);
    begin == cursor || is_whitespace_only(text, begin, cursor)
}

/// True when the cursor sits before a blank line tail or whitespace-only span.
pub(super) fn should_delete_by_char_forward(text: &str, cursor: usize) -> bool {
    let end = line_end(text, cursor);
    end == cursor || is_whitespace_only(text, cursor, end)
}

fn is_whitespace_only(text: &str, start: usize, end: usize) -> bool {
    start < end && text[start..end].chars().all(|c| c.is_whitespace())
}

pub(super) fn prev_char_index(text: &str, index: usize) -> usize {
    text[..index].char_indices().last().map_or(0, |(i, _)| i)
}

fn is_word_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}
