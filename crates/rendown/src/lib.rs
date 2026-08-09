//! Streaming CommonMark/markdown renderer for terminal (ANSI).
//!
//! Cloned pipeline from `elph-tui` markdown (neutral colors, no iocraft). Used by
//! headless `elph run --output=pretty`.

mod ansi;
mod blocks;
mod colors;
mod highlight;
mod layout;
mod linkify;
mod model;
mod parse;
mod parser_config;
mod stream;
mod syntax;
mod table;
mod theme;
mod wrap;

pub use ansi::{document_to_plain, write_document_ansi};
pub use layout::markdown_document_row_count;
pub use model::{FontWeight, MarkdownDocument, MarkdownLine, MarkdownLineKind, MarkdownTable, RgbColor, StyledSpan};
pub use parse::{parse_markdown_document, parse_markdown_document_with_theme};
pub use parser_config::has_open_container_at;
pub use stream::{StreamRenderer, terminal_width};
pub use theme::MarkdownTheme;

/// Parse markdown with the default theme.
pub fn parse_markdown(source: &str) -> MarkdownDocument {
    parse_markdown_document(source)
}

/// Parse markdown with an explicit theme.
pub fn parse_markdown_with_theme(source: &str, theme: &MarkdownTheme) -> MarkdownDocument {
    parse_markdown_document_with_theme(source, theme)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_inline_styles_and_code_fence() {
        let doc = parse_markdown("**Hi** and `x`\n\n```rust\nfn main() {}\nlet x = 1;\n```");
        assert!(doc.lines.len() >= 2);
        assert!(doc.lines.iter().any(|line| {
            line.spans
                .iter()
                .any(|span| span.weight == FontWeight::Bold && span.text.contains("Hi"))
        }));
        assert!(doc.lines.iter().any(|line| line.code_background));
    }

    #[test]
    fn plain_text_export() {
        let doc = parse_markdown("# Title\n\nBody");
        let plain = document_to_plain(&doc);
        assert!(plain.contains("Title"));
        assert!(plain.contains("Body"));
    }

    #[test]
    fn ansi_write_contains_text() {
        let doc = parse_markdown("- item one\n- item two");
        let mut buf = Vec::new();
        write_document_ansi(&doc, 80, &MarkdownTheme::default(), &mut buf).unwrap();
        let s = String::from_utf8_lossy(&buf);
        assert!(s.contains("item one"));
        assert!(s.contains("item two"));
    }
}
