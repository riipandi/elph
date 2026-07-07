use super::width::{char_display_width, str_display_width};

const ELLIPSIS: &str = "...";

/// Truncates `text` to at most `max_width` display columns, appending `ellipsis` when truncated.
pub fn truncate_to_width(text: &str, max_width: usize, ellipsis: &str) -> String {
    if max_width == 0 {
        return String::new();
    }
    if str_display_width(text) <= max_width {
        return text.to_string();
    }

    let ellipsis_width = str_display_width(ellipsis);
    let target = max_width.saturating_sub(ellipsis_width);
    if target == 0 {
        return ellipsis.chars().take(max_width).collect();
    }

    let mut out = String::new();
    let mut col = 0usize;
    let mut in_escape = false;
    let mut escape_buf = String::new();

    for ch in text.chars() {
        if in_escape {
            escape_buf.push(ch);
            if ch.is_ascii_alphabetic() {
                in_escape = false;
                let w = str_display_width(&escape_buf);
                if col + w > target {
                    out.push_str(ellipsis);
                    return out;
                }
                out.push_str(&escape_buf);
                col += w;
                escape_buf.clear();
            }
            continue;
        }

        if ch == '\x1b' {
            in_escape = true;
            escape_buf.push(ch);
            continue;
        }

        let w = char_display_width(ch, col);
        if col + w > target {
            out.push_str(ellipsis);
            return out;
        }
        out.push(ch);
        col += w;
    }

    out
}

/// Truncates without an ellipsis suffix.
pub fn truncate_to_width_no_ellipsis(text: &str, max_width: usize) -> String {
    truncate_to_width(text, max_width, "")
}

/// Truncates with the Unicode ellipsis character ([`ELLIPSIS`]).
pub fn truncate_to_width_ellipsis(text: &str, max_width: usize) -> String {
    truncate_to_width(text, max_width, ELLIPSIS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncates_plain_text() {
        assert_eq!(truncate_to_width("Hello World", 8, ELLIPSIS), "Hello...");
    }

    #[test]
    fn preserves_ansi_prefix() {
        let styled = "\x1b[31mHello\x1b[0m World".to_string();
        let truncated = truncate_to_width(&styled, 8, "...");
        assert!(truncated.contains("\x1b[31m"));
        assert!(str_display_width(&truncated) <= 8);
    }
}
