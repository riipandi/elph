use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// Terminal tab width used for layout and rendering.
pub const TAB_STOP: usize = 8;

/// Display width of a grapheme cluster (tabs advance to the next stop at `col`).
pub fn grapheme_display_width(g: &str, col: usize) -> usize {
    if g == "\t" {
        return TAB_STOP - (col % TAB_STOP);
    }
    if g == "\r" {
        return 0;
    }
    UnicodeWidthStr::width(g)
}

/// Display width of a character at the given column (tabs advance to the next stop).
pub fn char_display_width(ch: char, col: usize) -> usize {
    grapheme_display_width(&ch.to_string(), col)
}

/// Total display width of a string (ignores ANSI escape sequences).
pub fn str_display_width(s: &str) -> usize {
    let mut col = 0usize;
    let mut in_escape = false;
    for g in s.graphemes(true) {
        if in_escape {
            if g.chars().any(|ch| ch.is_ascii_alphabetic()) {
                in_escape = false;
            }
            continue;
        }
        if g == "\x1b" {
            in_escape = true;
            continue;
        }
        col += grapheme_display_width(g, col);
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

    for (_, cluster) in text.grapheme_indices(true) {
        if in_escape {
            escape.push_str(cluster);
            if cluster.chars().any(|ch| ch.is_ascii_alphabetic() || ch == '\x07') {
                in_escape = false;
                if col >= start && col < end {
                    out.push_str(&escape);
                }
                escape.clear();
            }
            continue;
        }

        if cluster == "\x1b" {
            in_escape = true;
            escape.push_str(cluster);
            continue;
        }

        let w = grapheme_display_width(cluster, col);
        if col + w > start && col < end {
            out.push_str(cluster);
        }
        col += w;
        if col >= end {
            break;
        }
    }

    out
}
