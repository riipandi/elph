//! Markdown pipeline: rendown parse/highlight + iocraft paint.

mod blocks;
pub(crate) mod convert;
mod layout;
mod render;
mod table;
mod theme;

pub use layout::{markdown_document_row_count, markdown_source_row_count};
pub use render::{plain_text_document, render_linkified_plain_text, render_markdown_block, render_markdown_children};
pub use render::{render_markdown_document, render_markdown_lines, streaming_tail_document};
pub use rendown::{MarkdownDocument, MarkdownLine, MarkdownLineKind, MarkdownTable, MarkdownTheme, StyledSpan};
pub use rendown::{has_open_container_at as markdown_has_open_container_at, path_to_file_url, spans_with_links};

use super::scroll_box::ScrollBox;
use super::theme::{UiTheme, resolve_ui_theme};
use iocraft::prelude::*;
use rendown::Rendown;

/// Parse markdown with the default theme.
pub fn parse_markdown_document(source: &str) -> MarkdownDocument {
    Rendown::new().parse(source)
}

/// Parse markdown with an explicit theme.
pub fn parse_markdown_document_with_theme(source: &str, theme: &MarkdownTheme) -> MarkdownDocument {
    Rendown::new().theme(*theme).parse(source)
}

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
    let markdown_theme = theme::theme_from_ui(ui_theme);
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
    fn render_document_produces_elements() {
        let doc = parse_markdown_document("Hello **world**");
        let elements = render_markdown_document(&doc);
        assert!(!elements.is_empty());
    }
}
