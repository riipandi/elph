/// Strip ANSI CSI/OSC escape sequences from terminal-styled text.
pub fn strip_ansi(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '\x1b' {
            out.push(ch);
            continue;
        }

        match chars.peek().copied() {
            Some('[') => {
                chars.next();
                for c in chars.by_ref() {
                    if c.is_ascii_alphabetic() || c == '\x07' {
                        break;
                    }
                }
            }
            Some(']') => {
                chars.next();
                while let Some(c) = chars.next() {
                    if c == '\x07' {
                        break;
                    }
                    if c == '\x1b' && chars.peek() == Some(&'\\') {
                        chars.next();
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::strip_ansi;

    #[test]
    fn strips_256_color_foreground() {
        let styled = "\x1b[38;5;252mHello\x1b[0m world".to_string();
        assert_eq!(strip_ansi(&styled), "Hello world");
    }

    #[test]
    fn preserves_plain_text() {
        assert_eq!(strip_ansi("what's different"), "what's different");
    }

    #[test]
    fn strips_osc_hyperlink() {
        let linked = "\x1b]8;;https://example.com\x1b\\link\x1b]8;;\x1b\\".to_string();
        assert_eq!(strip_ansi(&linked), "link");
    }
}
