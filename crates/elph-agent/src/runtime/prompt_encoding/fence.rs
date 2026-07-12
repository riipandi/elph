//! TOON markdown fence formatting and parsing.

use super::config::{DEFAULT_PREAMBLE, PromptEncodingDelimiter};

pub(crate) const TOON_FENCE_OPEN: &str = "```toon";

/// Build the model-visible block: optional preamble + fenced TOON body.
pub(crate) fn format_toon_block(encoded: &str, preamble: Option<&str>, delimiter: PromptEncodingDelimiter) -> String {
    let preamble = preamble
        .filter(|s| !s.is_empty())
        .map(|base| append_delimiter_hint(base, delimiter))
        .unwrap_or_else(|| append_delimiter_hint(DEFAULT_PREAMBLE, delimiter));

    let mut out = String::new();
    out.push_str(&preamble);
    out.push_str("\n\n");
    out.push_str(TOON_FENCE_OPEN);
    out.push('\n');
    out.push_str(encoded);
    if !encoded.ends_with('\n') {
        out.push('\n');
    }
    out.push_str("```");
    out
}

fn append_delimiter_hint(preamble: &str, delimiter: PromptEncodingDelimiter) -> String {
    if delimiter == PromptEncodingDelimiter::Tab && !preamble.contains("tab-separated") {
        format!("{preamble} Fields are tab-separated.")
    } else {
        preamble.to_string()
    }
}

/// Extract the TOON body from a fenced block, if present.
pub fn parse_toon_fence(text: &str) -> Option<&str> {
    let start_marker = "```toon\n";
    let start = text.find(start_marker)?;
    let body_start = start + start_marker.len();
    let closing = text[body_start..].find("\n```")?;
    let body_end = body_start + closing;
    let body = &text[body_start..body_end];
    if body.is_empty() {
        return None;
    }
    Some(body)
}

pub(crate) fn is_toon_encoded(text: &str) -> bool {
    parse_toon_fence(text).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_and_parse_roundtrip() {
        let body = "items[2]{id,name}:\n  1,a\n  2,b";
        let block = format_toon_block(body, Some("Data is in TOON format."), PromptEncodingDelimiter::Comma);
        assert!(block.contains("```toon"));
        assert_eq!(parse_toon_fence(&block), Some(body));
    }

    #[test]
    fn tab_delimiter_adds_hint() {
        let block = format_toon_block("x: 1", None, PromptEncodingDelimiter::Tab);
        assert!(block.contains("tab-separated"));
    }

    #[test]
    fn rejects_empty_fence_body() {
        let text = "preamble\n\n```toon\n\n```";
        assert!(parse_toon_fence(text).is_none());
    }
}
