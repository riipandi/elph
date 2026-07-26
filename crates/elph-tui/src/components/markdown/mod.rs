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
mod table;
mod theme;

pub use layout::{markdown_document_row_count, markdown_source_row_count};
pub use linkify::{path_to_file_url, spans_with_links};
pub use model::{MarkdownDocument, MarkdownLine, MarkdownLineKind, MarkdownTable, StyledSpan};
pub use parse::{parse_markdown_document, parse_markdown_document_with_theme};
pub use parser_config::has_open_container_at as markdown_has_open_container_at;
pub use render::{plain_text_document, render_linkified_plain_text, render_markdown_block, render_markdown_children};
pub use render::{render_markdown_document, render_markdown_lines, streaming_tail_document};
pub use theme::MarkdownTheme;

use super::scroll_box::ScrollBox;
use super::theme::{UiTheme, resolve_ui_theme};
use iocraft::prelude::*;

/// Props for [`MarkdownView`].
#[derive(Clone, Default, Props)]
pub struct MarkdownViewProps {
    pub width: u16,
    pub height: u16,
    pub source: String,
    pub theme: Option<UiTheme>,
}

/// Scrollable markdown document.
#[component]
pub fn MarkdownView(props: &MarkdownViewProps, hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let ui_theme = resolve_ui_theme(&hooks, props.theme);
    let markdown_theme = MarkdownTheme::from_ui_theme(ui_theme);
    let document = parse_markdown_document_with_theme(&props.source, &markdown_theme);
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
        let doc = parse_markdown_document("**Hi** and `x`\n\n```rust\nfn main() {}\nlet x = 1;\n```");
        assert!(doc.lines.len() >= 2);
        assert!(doc.lines.iter().any(|line| {
            line.spans
                .iter()
                .any(|span| span.weight == iocraft::prelude::Weight::Bold && span.text.contains("Hi"))
        }));
        assert!(doc.lines.iter().any(|line| line.code_background));
        let single = parse_markdown_document("```rust\nfn main() {}\n```");
        let code = single
            .lines
            .iter()
            .find(|line| line.kind == MarkdownLineKind::Code)
            .expect("single-line fence");
        assert!(!code.code_background);
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
        assert!(!url_span.underline, "links must not paint underline");
        assert_eq!(url_span.href.as_deref(), Some("https://elph.space"));
    }

    #[test]
    fn markdown_link_uses_destination_as_href() {
        let doc = parse_markdown_document("[docs](https://elph.space/guide)");
        let line = doc.lines.first().expect("paragraph line");
        let link_span = line
            .spans
            .iter()
            .find(|span| span.text.contains("docs"))
            .expect("link label span");
        assert!(!link_span.underline, "links must not paint underline");
        assert_eq!(link_span.href.as_deref(), Some("https://elph.space/guide"));
        assert_eq!(link_span.color, MarkdownTheme::default().link);
    }

    #[test]
    fn plain_path_gets_file_href() {
        let doc = parse_markdown_document("open /tmp/demo/file.rs please");
        let line = doc.lines.first().expect("paragraph line");
        let path_span = line
            .spans
            .iter()
            .find(|span| span.text.contains("file.rs"))
            .expect("path span");
        assert!(!path_span.underline, "paths must not paint underline");
        assert!(
            path_span
                .href
                .as_deref()
                .is_some_and(|h| h.starts_with("file://") && h.contains("file.rs")),
            "href={:?}",
            path_span.href
        );
    }
}
