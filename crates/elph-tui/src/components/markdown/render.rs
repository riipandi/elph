//! Fast paint path: cached [`MarkdownDocument`] → iocraft elements.

use iocraft::prelude::*;

use super::blocks::{code_content_width, segment_end, segment_gap_after};
use super::linkify::spans_with_links;
use super::model::{MarkdownDocument, MarkdownLine, MarkdownLineKind, StyledSpan};
use super::theme::MarkdownTheme;

fn span_to_mixed(span: &StyledSpan) -> MixedTextContent {
    let mut part = MixedTextContent::new(span.text.as_str()).color(span.color);
    if span.weight == Weight::Bold {
        part = part.weight(Weight::Bold);
    }
    if span.italic {
        part = part.italic();
    }
    part
}

fn line_to_mixed_contents(line: &MarkdownLine) -> Vec<MixedTextContent> {
    line.spans.iter().map(span_to_mixed).collect()
}

fn render_mixed_line(line: &MarkdownLine, width: u16, wrap: TextWrap, margin_bottom: u16) -> AnyElement<'static> {
    let contents = line_to_mixed_contents(line);
    element! {
        View(width: width, margin_bottom: margin_bottom, flex_shrink: 0f32) {
            MixedText(contents: contents, wrap: wrap)
        }
    }
    .into()
}

fn render_code_block(
    lines: &[MarkdownLine],
    width: u16,
    theme: &MarkdownTheme,
    margin_bottom: u16,
) -> AnyElement<'static> {
    let inner_width = code_content_width(width);
    let row_elements: Vec<AnyElement<'static>> = lines
        .iter()
        .map(|line| {
            element! {
                View(width: inner_width, flex_shrink: 0f32) {
                    MixedText(
                        contents: line_to_mixed_contents(line),
                        wrap: TextWrap::Wrap,
                    )
                }
            }
            .into()
        })
        .collect();
    element! {
        View(
            width: width,
            margin_bottom: margin_bottom,
            background_color: theme.code_bg,
            padding_left: 1,
            padding_right: 1,
            padding_top: 1,
            padding_bottom: 1,
            flex_direction: FlexDirection::Column,
            gap: 0,
            flex_shrink: 0f32,
        ) {
            #(row_elements)
        }
    }
    .into()
}

fn render_list_block(lines: &[MarkdownLine], width: u16, margin_bottom: u16) -> AnyElement<'static> {
    let items: Vec<AnyElement<'static>> = lines
        .iter()
        .map(|line| render_mixed_line(line, width, TextWrap::Wrap, 0))
        .collect();
    element! {
        View(
            width: width,
            margin_bottom: margin_bottom,
            flex_direction: FlexDirection::Column,
            gap: 0,
            flex_shrink: 0f32,
        ) {
            #(items)
        }
    }
    .into()
}

/// Build child elements for a document with explicit wrap width (iocraft measure path).
pub fn render_markdown_children(document: &MarkdownDocument, width: u16) -> Vec<AnyElement<'static>> {
    render_markdown_children_with_theme(document, width, &MarkdownTheme::default())
}

pub fn render_markdown_children_with_theme(
    document: &MarkdownDocument,
    width: u16,
    theme: &MarkdownTheme,
) -> Vec<AnyElement<'static>> {
    let width = width.max(1);
    let lines = &document.lines;
    let mut children = Vec::new();
    let mut index = 0usize;
    while index < lines.len() {
        let end = segment_end(lines, index);
        let gap = segment_gap_after(lines, index, end);
        let line = &lines[index];
        if line.is_blank() {
            children.push(
                element! {
                    View(width: width, height: 1, flex_shrink: 0f32)
                }
                .into(),
            );
            index = end;
            continue;
        }

        if line.code_background || line.kind == MarkdownLineKind::Code {
            children.push(render_code_block(&lines[index..end], width, theme, gap));
            index = end;
            continue;
        }

        if line.kind == MarkdownLineKind::ListItem {
            children.push(render_list_block(&lines[index..end], width, gap));
            index = end;
            continue;
        }

        children.push(render_mixed_line(line, width, TextWrap::Wrap, gap));
        index = end;
    }
    children
}

/// Render a full markdown block inside one column `View` (preferred for transcript cards).
pub fn render_markdown_block(document: &MarkdownDocument, width: u16) -> AnyElement<'static> {
    render_markdown_block_with_theme(document, width, &MarkdownTheme::default())
}

pub fn render_markdown_block_with_theme(
    document: &MarkdownDocument,
    width: u16,
    theme: &MarkdownTheme,
) -> AnyElement<'static> {
    let width = width.max(1);
    if document.is_empty() {
        return element! {
            View(width: width, flex_shrink: 0f32) {
                Text(content: "", color: theme.body)
            }
        }
        .into();
    }
    let children = render_markdown_children_with_theme(document, width, theme);
    element! {
        View(
            width: width,
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::FlexStart,
            gap: 0,
            flex_shrink: 0f32,
        ) {
            #(children)
        }
    }
    .into()
}

/// Parse streaming tail as markdown (code fences, lists, inline styles).
pub fn streaming_tail_document(text: &str) -> MarkdownDocument {
    if text.is_empty() {
        return MarkdownDocument::default();
    }
    super::parse::parse_markdown_document(text)
}

/// Convert plain/unparsed source into linkified document lines (streaming tail).
pub fn plain_text_document(text: &str, foreground: Color) -> MarkdownDocument {
    let theme = MarkdownTheme::default();
    if text.is_empty() {
        return MarkdownDocument::default();
    }
    let mut lines = Vec::new();
    for paragraph in text.split("\n\n") {
        if paragraph.is_empty() {
            lines.push(MarkdownLine::blank());
            continue;
        }
        let paragraph_lines: Vec<&str> = paragraph.lines().collect();
        for (index, line) in paragraph_lines.iter().enumerate() {
            let is_last_in_paragraph = index + 1 == paragraph_lines.len();
            let kind = if is_last_in_paragraph {
                MarkdownLineKind::Paragraph
            } else {
                MarkdownLineKind::Continuation
            };
            lines.push(MarkdownLine {
                kind,
                spans: spans_with_links(line, foreground, Weight::Normal, false, theme.link),
                code_background: false,
            });
        }
    }
    if lines.is_empty() {
        lines.push(MarkdownLine {
            kind: MarkdownLineKind::Paragraph,
            spans: spans_with_links(text, foreground, Weight::Normal, false, theme.link),
            code_background: false,
        });
    }
    MarkdownDocument { lines }.normalize()
}

/// Convert a cached document into iocraft elements (UI thread only).
pub fn render_markdown_document(document: &MarkdownDocument) -> Vec<AnyElement<'static>> {
    vec![render_markdown_block(document, 80)]
}

/// Legacy API used by [`super::MarkdownView`] and existing tests.
pub fn render_markdown_lines(source: &str) -> Vec<AnyElement<'static>> {
    let document = super::parse::parse_markdown_document(source);
    vec![render_markdown_block(&document, 80)]
}

/// Render unparsed plain text with auto-detected links (streaming tail / parse fallback).
pub fn render_linkified_plain_text(text: &str, foreground: Color, width: u16) -> AnyElement<'static> {
    if text.is_empty() {
        return element! { View(width: width.max(1)) }.into();
    }
    let document = plain_text_document(text, foreground);
    render_markdown_block_with_theme(&document, width, &MarkdownTheme::default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::markdown::{markdown_document_row_count, parse_markdown_document};

    #[test]
    fn code_block_groups_into_single_background_view() {
        let doc = parse_markdown_document("```rust\nlet a = 1;\nlet b = 2;\n```");
        let block = render_markdown_block(&doc, 60);
        let rendered = element! { View(width: 60) { #(vec![block]) } }.to_string();
        assert!(rendered.contains("let a = 1;"));
        assert!(rendered.contains("let b = 2;"));
    }

    #[test]
    fn code_block_wraps_long_lines() {
        let long = "x".repeat(80);
        let doc = parse_markdown_document(&format!("```\n{long}\n```"));
        let rows = markdown_document_row_count(&doc, 40);
        assert!(rows >= 4, "long code should wrap to multiple rows, got {rows}");
    }

    #[test]
    fn block_respects_wrap_width() {
        let doc = parse_markdown_document("hello world");
        let narrow = element! { View(width: 8) { #(vec![render_markdown_block(&doc, 8)]) } }.to_string();
        assert!(narrow.lines().count() >= 2);
    }

    #[test]
    fn plain_text_single_newline_uses_continuation() {
        let doc = plain_text_document("line one\nline two", Color::Reset);
        assert_eq!(doc.lines.len(), 2);
        assert_eq!(doc.lines[0].kind, MarkdownLineKind::Continuation);
        assert_eq!(doc.lines[1].kind, MarkdownLineKind::Paragraph);
    }

    #[test]
    fn streaming_tail_parses_unclosed_fence_as_code() {
        let doc = streaming_tail_document("```rust\nlet x = 1;");
        assert!(doc.lines.iter().any(|line| line.code_background));
    }
}
