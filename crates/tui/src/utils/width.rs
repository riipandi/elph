use unicode_width::UnicodeWidthChar;

/// Terminal tab width used for layout and rendering.
pub const TAB_STOP: usize = 8;

/// Display width of a character at the given column (tabs advance to the next stop).
pub fn char_display_width(ch: char, col: usize) -> usize {
    match ch {
        '\t' => TAB_STOP - (col % TAB_STOP),
        '\r' => 0,
        ch => ch.width().unwrap_or(0),
    }
}

/// Total display width of a string (ignores ANSI escape sequences).
pub fn str_display_width(s: &str) -> usize {
    let mut col = 0usize;
    let mut in_escape = false;
    for ch in s.chars() {
        if in_escape {
            if ch.is_ascii_alphabetic() {
                in_escape = false;
            }
            continue;
        }
        if ch == '\x1b' {
            in_escape = true;
            continue;
        }
        col += char_display_width(ch, col);
    }
    col
}

/// Returns the substring spanning display columns `[start, start + len)`.
pub fn slice_display_columns(text: &str, start: usize, len: usize) -> String {
    if len == 0 {
        return String::new();
    }

    let mut out = String::new();
    let mut col = 0usize;
    let mut in_escape = false;
    let mut escape = String::new();
    let end = start.saturating_add(len);

    for ch in text.chars() {
        if in_escape {
            escape.push(ch);
            if ch.is_ascii_alphabetic() || ch == '\x07' {
                in_escape = false;
                if col >= start && col < end {
                    out.push_str(&escape);
                }
                escape.clear();
            }
            continue;
        }

        if ch == '\x1b' {
            in_escape = true;
            escape.push(ch);
            continue;
        }

        let w = char_display_width(ch, col);
        if col + w > start && col < end {
            out.push(ch);
        }
        col += w;
        if col >= end {
            break;
        }
    }

    out
}
