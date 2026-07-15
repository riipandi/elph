//! Text layout helpers.

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// Word-wrap plain text to fit `max_width` display columns.
pub fn wrap_text(text: &str, max_width: usize) -> Vec<String> {
    let max_width = max_width.max(1);
    let mut lines = Vec::new();

    for paragraph in text.split('\n') {
        if paragraph.is_empty() {
            lines.push(String::new());
            continue;
        }

        let mut current = String::new();
        let mut current_width = 0;

        for word in paragraph.split_whitespace() {
            let word_width = word.width();
            let extra = if current.is_empty() { 0 } else { 1 };
            if current_width + extra + word_width > max_width {
                if !current.is_empty() {
                    lines.push(current);
                    current = String::new();
                    current_width = 0;
                }
                if word_width > max_width {
                    for chunk in chunk_graphemes(word, max_width) {
                        lines.push(chunk);
                    }
                    continue;
                }
            }
            if !current.is_empty() {
                current.push(' ');
                current_width += 1;
            }
            current.push_str(word);
            current_width += word_width;
        }

        if !current.is_empty() {
            lines.push(current);
        }
    }

    if lines.is_empty() {
        lines.push(String::new());
    }

    lines
}

fn chunk_graphemes(text: &str, max_width: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current = String::new();
    let mut width = 0;

    for g in text.graphemes(true) {
        let g_width = g.width();
        if width + g_width > max_width && !current.is_empty() {
            chunks.push(current);
            current = String::new();
            width = 0;
        }
        current.push_str(g);
        width += g_width;
    }

    if !current.is_empty() {
        chunks.push(current);
    }

    chunks
}

/// Truncate text to `max_width` display columns with an ellipsis suffix.
pub fn truncate_with_ellipsis(text: &str, max_width: usize) -> String {
    if max_width == 0 {
        return String::new();
    }
    if text.width() <= max_width {
        return text.to_string();
    }
    if max_width <= 1 {
        return "…".to_string();
    }

    let target = max_width - 1;
    let mut out = String::new();
    let mut width = 0;
    for g in text.graphemes(true) {
        let g_width = g.width();
        if width + g_width > target {
            break;
        }
        out.push_str(g);
        width += g_width;
    }
    out.push('…');
    out
}
