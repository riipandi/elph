use super::truncate::truncate_to_width_no_ellipsis;
use super::width::{char_display_width, str_display_width};

/// Word-wraps plain text to `max_width` display columns (no ANSI preservation).
pub fn wrap_text(text: &str, max_width: usize) -> Vec<String> {
    let max_width = max_width.max(1);
    if text.is_empty() {
        return vec![String::new()];
    }

    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        if paragraph.is_empty() {
            lines.push(String::new());
            continue;
        }
        lines.extend(wrap_paragraph(paragraph, max_width));
    }
    lines
}

fn wrap_paragraph(text: &str, max_width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    let mut col = 0usize;

    for ch in text.chars() {
        let w = char_display_width(ch, col);
        if col > 0 && col + w > max_width {
            lines.push(std::mem::take(&mut current));
            col = 0;
        }
        current.push(ch);
        col += char_display_width(ch, col.saturating_sub(w));
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
    for paragraph in text.split('\n') {
        if paragraph.is_empty() {
            lines.push(String::new());
            continue;
        }
        lines.extend(wrap_ansi_line(paragraph, max_width));
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
    fn wraps_ansi_line_preserving_color() {
        let line = format!("\x1b[31mhello\x1b[0m world foo");
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
}
