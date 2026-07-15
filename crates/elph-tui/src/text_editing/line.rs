//! Line and word boundary helpers (byte offsets into UTF-8).

/// Returns true for characters treated as part of a word (GUI-style).
pub fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Start of the line containing `cursor` (byte offset).
pub fn line_start_offset(text: &str, cursor: usize) -> usize {
    text[..cursor.min(text.len())].rfind('\n').map(|i| i + 1).unwrap_or(0)
}

/// End of the line containing `cursor` (byte offset, before `\n` or EOF).
pub fn line_end_offset(text: &str, cursor: usize) -> usize {
    let cursor = cursor.min(text.len());
    text[cursor..].find('\n').map(|i| cursor + i).unwrap_or(text.len())
}

/// Byte offset of the previous word boundary (macOS Option+← / Linux Ctrl+←).
pub fn prev_word_offset(text: &str, cursor: usize) -> usize {
    let mut i = cursor.min(text.len());
    let line_start = line_start_offset(text, cursor);
    if i <= line_start {
        return line_start;
    }

    while i > line_start {
        let Some(ch) = text[..i].chars().next_back() else { break };
        if is_word_char(ch) {
            break;
        }
        i -= ch.len_utf8();
    }

    while i > line_start {
        let Some(ch) = text[..i].chars().next_back() else { break };
        if !is_word_char(ch) {
            break;
        }
        i -= ch.len_utf8();
    }

    i
}

/// Byte offset of the next word boundary (macOS Option+→ / Linux Ctrl+→).
pub fn next_word_offset(text: &str, cursor: usize) -> usize {
    let mut i = cursor.min(text.len());
    let line_end = line_end_offset(text, cursor);

    while i < line_end {
        let Some(ch) = text[i..].chars().next() else { break };
        if !is_word_char(ch) {
            break;
        }
        i += ch.len_utf8();
    }

    while i < line_end {
        let Some(ch) = text[i..].chars().next() else { break };
        if is_word_char(ch) {
            break;
        }
        i += ch.len_utf8();
    }

    i
}
