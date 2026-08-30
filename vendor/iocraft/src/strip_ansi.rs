use regex::Regex;
use std::{borrow::Cow, sync::LazyLock};

pub const ANSI_REGEX_PATTERN: &str = concat!(
    // OSC branch
    "(?:\\x1B\\][^\\x07\\x1B\\x9C]*?(?:\\x07|\\x1B\\\\|\\x9C))",
    "|",
    // DCS, APC, PM, and SOS sequences
    "(?:\\x1B[P_^X][^\\x1B\\x9C]*?(?:\\x1B\\\\|\\x9C))",
    "|",
    // CSI ESC[ ... (ECMA-48 parameter and intermediate byte classes)
    "(?:\\x1B\\[[\\x30-\\x3F]*[\\x20-\\x2F]*[\\x40-\\x7E])",
    "|",
    // CSI single-byte 0x9B ...
    "(?:\\x9B[\\x30-\\x3F]*[\\x20-\\x2F]*[\\x40-\\x7E])",
    "|",
    // VT52 / short escapes (single final)
    "(?:\\x1B[ABCDHIKJSTZ=><su78EMcNO])",
    "|",
    // Charset selection ESC (X or )X where X in A B 0 1 2
    "(?:\\x1B[()][AB012])",
    "|",
    // Hash sequences ESC # 3 4 5 6 8
    "(?:\\x1B#[34568])",
);

static ANSI_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(ANSI_REGEX_PATTERN).expect("valid ANSI regex"));

pub(crate) fn strip_ansi(string: &str) -> Cow<'_, str> {
    ANSI_REGEX.replace_all(string, "")
}

/// Strip ANSI sequences and remove control characters that can corrupt a raw-mode TUI.
///
/// Keeps `\n` and `\t` (layout). Drops `\r` and other C0/C1 controls so web tool bodies
/// (or any untrusted text) cannot reposition the cursor mid-paint.
pub(crate) fn sanitize_terminal_text(string: &str) -> Cow<'_, str> {
    let stripped = strip_ansi(string);
    if !stripped.chars().any(is_unsafe_terminal_control) {
        return stripped;
    }
    Cow::Owned(
        stripped
            .chars()
            .filter(|c| !is_unsafe_terminal_control(*c))
            .collect(),
    )
}

fn is_unsafe_terminal_control(c: char) -> bool {
    // Keep newline (line wrapping) and tab (expanded by layout consumers if needed).
    if c == '\n' || c == '\t' {
        return false;
    }
    c.is_control()
}

/// Max OSC 8 URI length written to the terminal (avoids huge / corrupt sequences).
pub(crate) const MAX_OSC8_URI_BYTES: usize = 2048;

/// Return a URI safe to embed in `OSC 8 ; ; URI ST`, or `None` to skip hyperlinking.
pub(crate) fn sanitize_osc8_uri(uri: &str) -> Option<&str> {
    // Validate before trimming so trailing controls cannot be silently removed and accepted.
    if uri
        .bytes()
        .any(|b| b < 0x20 || b == 0x7f || b == 0x1b || b == 0x9c || b == 0x07)
    {
        return None;
    }
    let uri = uri.trim();
    if uri.is_empty() || uri.len() > MAX_OSC8_URI_BYTES {
        return None;
    }
    Some(uri)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_drops_carriage_return() {
        let out = sanitize_terminal_text("a\rb\nc");
        assert_eq!(out.as_ref(), "ab\nc");
    }

    #[test]
    fn sanitize_keeps_plain_text() {
        let out = sanitize_terminal_text("hello https://example.com");
        assert!(matches!(out, Cow::Borrowed(_)));
        assert_eq!(out.as_ref(), "hello https://example.com");
    }

    #[test]
    fn sanitize_strips_ansi_then_controls() {
        let out = sanitize_terminal_text("x\x1b[31mred\x1b[0m\ry");
        assert_eq!(out.as_ref(), "xredy");
    }

    #[test]
    fn osc8_rejects_control_bytes() {
        assert!(sanitize_osc8_uri("https://ok.example").is_some());
        assert!(sanitize_osc8_uri("https://bad.example/\x1b").is_none());
        assert!(sanitize_osc8_uri("https://bad.example/\r\n").is_none());
        assert!(sanitize_osc8_uri("").is_none());
    }

    #[test]
    fn strip_ansi_handles_ecma48_and_string_sequences() {
        assert_eq!(
            strip_ansi("a\x1b[38;2;1;2;3mred\x1b[0mb").as_ref(),
            "aredb"
        );
        assert_eq!(strip_ansi("a\x1bPignored\x1b\\b").as_ref(), "ab");
        assert_eq!(strip_ansi("a\x1b^ignored\x1b\\b").as_ref(), "ab");
    }

    #[test]
    fn strip_ansi_handles_long_csi_parameters() {
        let input = format!("a\x1b[{}mb", "1;".repeat(100));
        assert_eq!(strip_ansi(&input).as_ref(), "ab");
    }
}
