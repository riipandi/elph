//! Text layout helpers.

use std::iter::Peekable;
use std::str::Chars;

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// Normalize user prompt text before sticky-card wrap/clamp so width math and rendering stay clean.
pub fn sanitize_sticky_display_text(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\x1b' => skip_ansi_tail(&mut chars),
            '\u{009b}' => skip_csi_params(&mut chars),
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                out.push('\n');
            }
            '\n' => out.push('\n'),
            '\t' => out.push(' '),
            c if c.is_control() => {}
            c if is_invisible_format_char(c) => {}
            c => out.push(c),
        }
    }
    out
}

fn skip_ansi_tail(chars: &mut Peekable<Chars<'_>>) {
    match chars.next() {
        Some('[') => skip_csi_params(chars),
        Some(']') => {
            while let Some(ch) = chars.next() {
                if ch == '\x07' || ch == '\u{009c}' {
                    break;
                }
                if ch == '\x1b' && chars.next() == Some('\\') {
                    break;
                }
            }
        }
        Some('(') | Some(')') | Some('#') => {
            chars.next();
        }
        Some(_) => {}
        None => {}
    }
}

fn skip_csi_params(chars: &mut Peekable<Chars<'_>>) {
    for ch in chars.by_ref() {
        if ('@'..='~').contains(&ch) {
            break;
        }
    }
}

fn is_invisible_format_char(ch: char) -> bool {
    matches!(
        ch,
        '\u{00ad}' // soft hyphen
            | '\u{034f}' // CGJ
            | '\u{061c}' // ALM
            | '\u{180e}' // Mongolian vowel separator
            | '\u{200b}'..='\u{200f}' // ZWSP, ZWNJ, ZWJ, LRM, RLM
            | '\u{2028}'..='\u{2029}' // line/paragraph separators → drop (sticky uses explicit \n)
            | '\u{202a}'..='\u{202e}' // bidi embedding controls
            | '\u{2060}'..='\u{2064}' // word joiner, invisible operators
            | '\u{206a}'..='\u{206f}' // deprecated format controls
            | '\u{feff}' // BOM / ZWNBSP
            | '\u{fff9}'..='\u{fffb}' // interlinear annotation anchors
    )
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_sticky_display_text_strips_ansi_and_controls() {
        let raw = "\x1b[31mhello\x07world\x1b[0m";
        assert_eq!(sanitize_sticky_display_text(raw), "helloworld");
    }

    #[test]
    fn sanitize_sticky_display_text_normalizes_newlines_and_tabs() {
        assert_eq!(sanitize_sticky_display_text("a\r\nb\rc\td"), "a\nb\nc d");
    }

    #[test]
    fn sanitize_sticky_display_text_drops_zero_width_chars() {
        let raw = "hel\u{200b}lo\u{feff}";
        assert_eq!(sanitize_sticky_display_text(raw), "hello");
    }

    #[test]
    fn sanitize_sticky_display_text_is_idempotent() {
        let once = sanitize_sticky_display_text("plain\nline");
        assert_eq!(sanitize_sticky_display_text(&once), once);
    }
}
