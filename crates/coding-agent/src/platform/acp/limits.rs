//! Caps for ACP `session/update` payloads so a large tool dump cannot drop the client.

/// Max Unicode scalars in one agent/tool/terminal text update.
pub const MAX_UPDATE_CHARS: usize = 16_384;

pub fn truncate_text(text: &str) -> String {
    truncate_text_at(text, MAX_UPDATE_CHARS)
}

pub fn truncate_text_at(text: &str, max_chars: usize) -> String {
    let count = text.chars().count();
    if count <= max_chars {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max_chars).collect();
    out.push_str("\n…[truncated]");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_text_unchanged() {
        assert_eq!(truncate_text("ok"), "ok");
    }

    #[test]
    fn long_text_is_capped() {
        let big = "x".repeat(MAX_UPDATE_CHARS + 50);
        let out = truncate_text(&big);
        assert!(out.ends_with("…[truncated]"));
        assert!(out.chars().count() < big.chars().count());
        assert!(out.chars().count() <= MAX_UPDATE_CHARS + 20);
    }
}
