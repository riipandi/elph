//! ANSI styling for completed `@` file paths in the prompt editor.

use super::model::active_mention_at_cursor;
use crate::tui::theme::USER_INPUT_ACCENT;
use iocraft::prelude::Color;

fn mention_highlight_start() -> String {
    match USER_INPUT_ACCENT {
        Color::Rgb { r, g, b } => format!("\x1b[38;2;{r};{g};{b}m"),
        _ => String::from("\x1b[38;2;129;161;193m"),
    }
}

const MENTION_HIGHLIGHT_END: &str = "\x1b[0m";

fn is_completed_file_path(token: &str) -> bool {
    token.len() > 1 && token.as_bytes().get(1).is_some_and(|&b| b != b' ') && token[1..].contains('/')
}

/// Apply highlight to completed `@path` tokens; the active in-progress mention stays unstyled.
pub fn mention_highlight_ansi(text: &str, cursor: usize) -> String {
    if text.is_empty() {
        return String::new();
    }
    let active = active_mention_at_cursor(text, cursor);
    let mut out = String::with_capacity(text.len() + 32);
    let mut index = 0usize;
    while index < text.len() {
        if text[index..].starts_with('@') {
            if active.as_ref().is_some_and(|mention| mention.start == index) {
                let end = active.as_ref().map(|mention| mention.end).unwrap_or(index + 1);
                out.push_str(&text[index..end]);
                index = end;
                continue;
            }
            let start = index;
            index += 1;
            while index < text.len() {
                let ch = text[index..].chars().next().expect("char");
                if ch.is_whitespace() {
                    break;
                }
                index += ch.len_utf8();
            }
            let token = &text[start..index];
            if is_completed_file_path(token) {
                out.push_str(&mention_highlight_start());
                out.push_str(token);
                out.push_str(MENTION_HIGHLIGHT_END);
            } else {
                out.push_str(token);
            }
            continue;
        }
        let ch = text[index..].chars().next().expect("char");
        out.push(ch);
        index += ch.len_utf8();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlights_completed_mention_not_active_query() {
        let text = "see @src/main.rs and @lib";
        let styled = mention_highlight_ansi(text, text.len());
        assert!(styled.contains(&mention_highlight_start()));
        assert!(styled.contains("@src/main.rs"));
        assert!(!styled.contains(&format!("{}@lib", mention_highlight_start())));
    }

    #[test]
    fn in_progress_filter_query_stays_unstyled() {
        let text = "see @ma";
        let styled = mention_highlight_ansi(text, text.len());
        assert!(!styled.contains(&mention_highlight_start()));
        assert!(styled.contains("@ma"));
    }
}
