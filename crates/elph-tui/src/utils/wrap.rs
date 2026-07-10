use memchr::memchr;

use super::truncate::truncate_to_width_no_ellipsis;
use super::width::str_display_width;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// Word-wraps plain text to `max_width` display columns (no ANSI preservation).
pub fn wrap_text(text: &str, max_width: usize) -> Vec<String> {
    let max_width = max_width.max(1);
    if text.is_empty() {
        return vec![String::new()];
    }

    let mut lines = Vec::new();
    let mut start = 0usize;
    while start <= text.len() {
        let remaining = &text[start..];
        if remaining.is_empty() {
            break;
        }
        let (paragraph, next_start) = match memchr(b'\n', remaining.as_bytes()) {
            Some(end) => (&remaining[..end], start + end + 1),
            None => (remaining, text.len() + 1),
        };
        if paragraph.is_empty() {
            lines.push(String::new());
        } else {
            lines.extend(wrap_paragraph(paragraph, max_width));
        }
        if next_start > text.len() {
            break;
        }
        start = next_start;
    }
    lines
}

fn split_long_word(text: &str, max_width: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut chunk = String::new();
    let mut chunk_w = 0usize;

    for g in text.graphemes(true) {
        let w = UnicodeWidthStr::width(g);
        if chunk_w + w > max_width && !chunk.is_empty() {
            chunks.push(std::mem::take(&mut chunk));
            chunk_w = 0;
        }
        if w > max_width && chunk.is_empty() {
            chunks.push(g.to_string());
            continue;
        }
        chunk.push_str(g);
        chunk_w += w;
    }

    if !chunk.is_empty() {
        chunks.push(chunk);
    }
    chunks
}

fn wrap_paragraph(text: &str, max_width: usize) -> Vec<String> {
    let max_width = max_width.max(1);
    let mut lines = Vec::new();
    let mut current = String::new();
    let mut col = 0usize;

    for word in text.split_whitespace() {
        let word_width = UnicodeWidthStr::width(word);

        if word_width > max_width && col == 0 {
            lines.extend(split_long_word(word, max_width));
            current.clear();
            col = 0;
            continue;
        }

        let needs_space = col > 0;
        let next_width = col + if needs_space { 1 } else { 0 } + word_width;

        if needs_space && next_width > max_width {
            lines.push(std::mem::take(&mut current));
            col = 0;
        }

        if col == 0 && word_width > max_width {
            lines.extend(split_long_word(word, max_width));
            continue;
        }

        if col > 0 {
            current.push(' ');
            col += 1;
        }
        current.push_str(word);
        col += word_width;
    }

    if !current.is_empty() || lines.is_empty() {
        lines.push(current);
    }
    lines
}

/// Wraps a single ANSI-styled line to `max_width`, preserving escape sequences.
pub fn wrap_ansi_line(line: &str, max_width: usize) -> Vec<String> {
    let max_width = max_width.max(1);
    if line.is_empty() {
        return vec![String::new()];
    }
    if str_display_width(line) <= max_width {
        return vec![line.to_string()];
    }

    let words = split_ansi_words(line);
    let mut lines = Vec::new();
    let mut current = String::new();
    let mut col = 0usize;

    for word in words {
        let word_width = str_display_width(&word);
        let needs_space = col > 0;
        let next_width = col + if needs_space { 1 } else { 0 } + word_width;

        if needs_space && next_width > max_width {
            lines.push(std::mem::take(&mut current));
            col = 0;
        }

        if word_width > max_width && col == 0 {
            current.push_str(&truncate_to_width_no_ellipsis(&word, max_width));
            lines.push(std::mem::take(&mut current));
            continue;
        }

        if col > 0 {
            current.push(' ');
            col += 1;
        }
        current.push_str(&word);
        col += word_width;
    }

    if !current.is_empty() {
        lines.push(current);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

/// Splits on ASCII whitespace while keeping leading ANSI sequences on each token.
fn split_ansi_words(line: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut in_escape = false;
    let mut pending_ws = false;

    for ch in line.chars() {
        if in_escape {
            current.push(ch);
            if ch.is_ascii_alphabetic() || ch == '\x07' {
                in_escape = false;
            }
            continue;
        }

        if ch == '\x1b' {
            in_escape = true;
            current.push(ch);
            continue;
        }

        if ch.is_whitespace() {
            if !current.is_empty() {
                words.push(std::mem::take(&mut current));
            }
            pending_ws = true;
            continue;
        }

        if pending_ws && !current.is_empty() {
            words.push(std::mem::take(&mut current));
        }
        pending_ws = false;
        current.push(ch);
    }

    if !current.is_empty() {
        words.push(current);
    }
    words
}

/// Wraps multiline ANSI text paragraph-by-paragraph.
pub fn wrap_ansi_text(text: &str, max_width: usize) -> Vec<String> {
    let max_width = max_width.max(1);
    if text.is_empty() {
        return vec![String::new()];
    }

    let mut lines = Vec::new();
    let mut start = 0usize;
    while start <= text.len() {
        let remaining = &text[start..];
        if remaining.is_empty() {
            break;
        }
        let (paragraph, next_start) = match memchr(b'\n', remaining.as_bytes()) {
            Some(end) => (&remaining[..end], start + end + 1),
            None => (remaining, text.len() + 1),
        };
        if paragraph.is_empty() {
            lines.push(String::new());
        } else {
            lines.extend(wrap_ansi_line(paragraph, max_width));
        }
        if next_start > text.len() {
            break;
        }
        start = next_start;
    }
    lines
}

/// Applies horizontal padding to each line.
pub fn pad_lines(lines: &[String], padding_x: usize, padding_y: usize) -> Vec<String> {
    let pad = " ".repeat(padding_x);
    let mut out = Vec::new();
    for _ in 0..padding_y {
        out.push(String::new());
    }
    for line in lines {
        if padding_x == 0 {
            out.push(line.clone());
        } else {
            out.push(format!("{pad}{line}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_long_paragraph() {
        let lines = wrap_text("hello world foo", 6);
        assert!(lines.len() >= 2);
        assert!(lines.iter().all(|l| str_display_width(l) <= 6));
    }

    #[test]
    fn wraps_cjk_at_grapheme_boundaries() {
        let lines = wrap_text("日本語文字", 4);
        assert_eq!(lines, vec!["日本", "語文", "字"]);
    }

    #[test]
    fn wraps_sundanese_and_latin_mixed() {
        let lines = wrap_text("ᮘᮒᮞ aksara Sunda", 8);
        assert!(lines.len() >= 2);
        assert!(lines.iter().all(|l| str_display_width(l) <= 8));
    }

    #[test]
    fn preserves_combining_marks() {
        let input = "e\u{0301}llo world";
        let lines = wrap_text(input, 10);
        assert_eq!(lines.len(), 1);
        assert!(lines[0].contains('\u{0301}'));
    }

    #[test]
    fn wraps_ansi_line_preserving_color() {
        let line = "\x1b[31mhello\x1b[0m world foo".to_string();
        let wrapped = wrap_ansi_line(&line, 8);
        assert!(wrapped.len() >= 2);
        assert!(wrapped[0].contains("\x1b[31m"));
    }

    #[test]
    fn pad_lines_adds_margins() {
        let lines = pad_lines(&["a".into()], 2, 1);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0], "");
        assert_eq!(lines[1], "  a");
    }

    #[test]
    fn char_display_width_matches_grapheme() {
        use super::super::width::{char_display_width, grapheme_display_width};
        assert_eq!(char_display_width('日', 0), grapheme_display_width("日", 0));
    }
}
