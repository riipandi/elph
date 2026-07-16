//! Markdown pipeline: pulldown-cmark parse + syntect highlight + cached render.

mod blocks;
mod colors;
mod highlight;
mod layout;
mod linkify;
mod model;
mod parse;
mod parser_config;
mod render;
mod syntax;
mod theme;

pub use layout::{markdown_document_row_count, markdown_source_row_count};
pub use linkify::spans_with_links;
pub use model::{MarkdownDocument, MarkdownLine, MarkdownLineKind, StyledSpan};
pub use parse::{parse_markdown_document, parse_markdown_document_with_theme};
pub use parser_config::has_open_container_at as markdown_has_open_container_at;
pub use render::{
    plain_text_document, render_linkified_plain_text, render_markdown_block, render_markdown_children,
    render_markdown_document, render_markdown_lines, streaming_tail_document,
};
pub use theme::MarkdownTheme;

use super::scroll_box::ScrollBox;
use iocraft::prelude::*;

/// Props for [`MarkdownView`].
#[derive(Clone, Default, Props)]
pub struct MarkdownViewProps {
    pub width: u16,
    pub height: u16,
    pub source: String,
}

/// Scrollable markdown document.
#[component]
pub fn MarkdownView(props: &MarkdownViewProps) -> impl Into<AnyElement<'static>> {
    let document = parse_markdown_document(&props.source);
    let block = render_markdown_block(&document, props.width.max(1));

    element! {
        ScrollBox(
            width: props.width,
            height: props.height,
            children: vec![block],
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_inline_styles_and_code_fence() {
        let doc = parse_markdown_document("**Hi** and `x`\n\n```rust\nfn main() {}\n```");
        assert!(doc.lines.len() >= 2);
        assert!(doc.lines.iter().any(|line| {
            line.spans
                .iter()
                .any(|span| span.weight == iocraft::prelude::Weight::Bold && span.text.contains("Hi"))
        }));
        assert!(doc.lines.iter().any(|line| line.code_background));
    }

    #[test]
    fn document_row_count_is_positive() {
        let doc = parse_markdown_document("# Title\n\nBody");
        assert!(markdown_document_row_count(&doc, 40) >= 1);
    }

    #[test]
    fn render_document_produces_elements() {
        let doc = parse_markdown_document("Hello **world**");
        let elements = render_markdown_document(&doc);
        assert!(!elements.is_empty());
    }

    #[test]
    fn autolinks_urls_in_paragraph_text() {
        let doc = parse_markdown_document("See https://elph.space for docs");
        let line = doc.lines.first().expect("paragraph line");
        assert!(line.spans.iter().any(|span| span.text.contains("https://elph.space")));
        let url_span = line
            .spans
            .iter()
            .find(|span| span.text.contains("https://elph.space"))
            .expect("url span");
        assert_eq!(url_span.color, MarkdownTheme::default().link);
    }
}
