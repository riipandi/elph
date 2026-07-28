//! Fast paint path: cached [`MarkdownDocument`] → iocraft elements.

use iocraft::prelude::*;

use super::blocks::{CODE_BLOCK_INSET_H, CODE_BLOCK_INSET_V, code_content_width, segment_end, segment_gap_after};
use super::layout::wrap_with_hanging_ranges;
use super::linkify::spans_with_links;
use super::model::{MarkdownDocument, MarkdownLine, MarkdownLineKind, StyledSpan};
use super::table::render_markdown_table;
use super::theme::MarkdownTheme;

fn span_to_mixed(span: &StyledSpan) -> MixedTextContent {
    let mut part = MixedTextContent::new(span.text.as_str()).color(span.color);
    if span.weight == Weight::Bold {
        part = part.weight(Weight::Bold);
    }
    if span.italic {
        part = part.italic();
    }
    // Links stay clickable via OSC 8 / Cmd+click — do not paint underline.
    if span.underline {
        part = part.decoration(TextDecoration::Underline);
    }
    if let Some(href) = span.href.as_deref() {
        part = part.hyperlink(std::sync::Arc::<str>::from(href));
    }
    part
}

fn line_spans_to_mixed(span: &[StyledSpan]) -> Vec<MixedTextContent> {
    span.iter().map(span_to_mixed).collect()
}

/// Re-color the char range `[start, end)` of `spans`' concatenated text into styled spans.
///
/// Used to paint one visual row of a wrapped code line while keeping each token's color.
fn recolor_range(spans: &[StyledSpan], start: usize, end: usize) -> Vec<StyledSpan> {
    let mut out = Vec::new();
    let mut pos = 0usize;
    for span in spans {
        let slen = span.text.chars().count();
        let s_start = pos;
        let s_end = pos + slen;
        pos = s_end;
        if s_end <= start || s_start >= end {
            continue;
        }
        let cs = s_start.max(start);
        let ce = s_end.min(end);
        let char_start = cs - s_start;
        let char_end = ce - s_start;
        if char_end <= char_start {
            continue;
        }
        let sub: String = span.text.chars().skip(char_start).take(char_end - char_start).collect();
        if sub.is_empty() {
            continue;
        }
        out.push(StyledSpan {
            text: sub,
            color: span.color,
            weight: span.weight,
            italic: span.italic,
            underline: span.underline,
            href: span.href.clone(),
        });
    }
    out
}

/// Wrap a code line's styled spans to `inner` columns, preserving the line's leading whitespace
/// as a hanging indent on continuation rows. Each returned sub-vector is one visual row. Mirrors
/// [`super::layout::wrap_with_hanging_ranges`], which measures the same rows.
fn wrap_code_spans(spans: &[StyledSpan], inner: u16, body: Color) -> Vec<Vec<StyledSpan>> {
    let inner = inner.max(1);
    let plain: String = spans.iter().map(|s| s.text.as_str()).collect();
    let indent = plain
        .chars()
        .take_while(|c| *c == ' ')
        .count()
        .min((inner as usize).saturating_sub(1));
    let indent_str = " ".repeat(indent);
    let ranges = wrap_with_hanging_ranges(&plain, inner);
    ranges
        .iter()
        .enumerate()
        .map(|(ri, &(start, end))| {
            let mut row = recolor_range(spans, start, end);
            if ri > 0 {
                row.insert(0, StyledSpan::plain(indent_str.clone(), body));
            }
            row
        })
        .collect()
}

fn render_mixed_line(line: &MarkdownLine, width: u16, wrap: TextWrap, margin_bottom: u16) -> AnyElement<'static> {
    let contents = line_spans_to_mixed(&line.spans);
    element! {
        View(width: width, margin_bottom: margin_bottom, flex_shrink: 0f32) {
            MixedText(contents: contents, wrap: wrap)
        }
    }
    .into()
}

/// Render the wrapped visual rows of one parsed code line as non-wrapping `MixedText` views.
fn render_code_line_rows(line: &MarkdownLine, row_width: u16, theme: &MarkdownTheme) -> Vec<AnyElement<'static>> {
    wrap_code_spans(&line.spans, row_width, theme.body)
        .into_iter()
        .map(|row_spans| {
            element! {
                View(width: row_width, flex_shrink: 0f32) {
                    MixedText(contents: line_spans_to_mixed(&row_spans), wrap: TextWrap::NoWrap)
                }
            }
            .into()
        })
        .collect()
}

fn render_code_block(
    lines: &[MarkdownLine],
    width: u16,
    theme: &MarkdownTheme,
    margin_bottom: u16,
) -> AnyElement<'static> {
    let use_card = lines.iter().any(|line| line.code_background);
    if !use_card {
        // No card background (single-line fences): render wrapped rows inline at full width.
        let row_elements: Vec<AnyElement<'static>> = lines
            .iter()
            .flat_map(|line| render_code_line_rows(line, width, theme))
            .collect();
        return element! {
            View(
                width: width,
                margin_bottom: margin_bottom,
                flex_direction: FlexDirection::Column,
                gap: 0,
                flex_shrink: 0f32,
            ) {
                #(row_elements)
            }
        }
        .into();
    }

    let inner_width = code_content_width(width);
    let row_elements: Vec<AnyElement<'static>> = lines
        .iter()
        .flat_map(|line| render_code_line_rows(line, inner_width, theme))
        .collect();
    element! {
        View(
            width: width,
            margin_bottom: margin_bottom,
            background_color: theme.code_bg,
            padding_top: CODE_BLOCK_INSET_V,
            padding_bottom: CODE_BLOCK_INSET_V,
            padding_left: CODE_BLOCK_INSET_H,
            padding_right: CODE_BLOCK_INSET_H,
            flex_direction: FlexDirection::Column,
            gap: 0,
            flex_shrink: 0f32,
        ) {
            #(row_elements)
        }
    }
    .into()
}

fn render_table_block(
    line: &MarkdownLine,
    width: u16,
    theme: &MarkdownTheme,
    margin_bottom: u16,
) -> AnyElement<'static> {
    let table = line.table.as_ref().expect("table markdown line must carry table data");
    render_markdown_table(table, width, theme, margin_bottom).unwrap_or_else(|| {
        element! {
            View(width: width, margin_bottom: margin_bottom, flex_shrink: 0f32)
        }
        .into()
    })
}

fn markdown_horizontal_rule_text(width: u16) -> String {
    "─".repeat(width.max(1) as usize)
}

fn render_rule_line(width: u16, theme: &MarkdownTheme, margin_bottom: u16) -> AnyElement<'static> {
    element! {
        View(width: width, margin_bottom: margin_bottom, flex_shrink: 0f32) {
            Text(
                content: markdown_horizontal_rule_text(width),
                color: theme.horizontal_rule,
                wrap: TextWrap::NoWrap,
            )
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

        if line.kind == MarkdownLineKind::Table {
            children.push(render_table_block(line, width, theme, gap));
            index = end;
            continue;
        }

        if line.kind == MarkdownLineKind::Rule {
            children.push(render_rule_line(width, theme, gap));
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
                table: None,
            });
        }
    }
    if lines.is_empty() {
        lines.push(MarkdownLine {
            kind: MarkdownLineKind::Paragraph,
            spans: spans_with_links(text, foreground, Weight::Normal, false, theme.link),
            code_background: false,
            table: None,
        });
    }
    MarkdownDocument { lines }.normalize()
}

/// Convert a cached document into iocraft elements (UI thread only).
pub fn render_markdown_document(document: &MarkdownDocument) -> Vec<AnyElement<'static>> {
    vec![render_markdown_block(document, 80)]
}

/// Convenience API used by [`super::MarkdownView`] and existing tests.
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
    use crate::components::markdown::layout::code_line_row_count;
    use crate::components::markdown::{markdown_document_row_count, parse_markdown_document};

    #[test]
    fn wrap_code_spans_preserves_hanging_indent() {
        let body =
            "    let value = a_really_long_variable_name_that_exceeds_the_wrap_width_and_should_wrap_now = compute();";
        let spans = vec![StyledSpan::plain(body, Color::Reset)];
        let rows = wrap_code_spans(&spans, 24, Color::Reset);
        assert!(rows.len() >= 2, "long line should wrap: {rows:?}");
        // First row keeps the 4-space leading indent from the source line.
        let first: String = rows[0].iter().map(|s| s.text.as_str()).collect();
        assert!(first.starts_with("    let value ="), "first row: {first:?}");
        // Every continuation row must carry the same hanging indent.
        for (i, row) in rows.iter().enumerate().skip(1) {
            let text: String = row.iter().map(|s| s.text.as_str()).collect();
            assert!(text.starts_with("    "), "continuation {i} not indented: {text:?}");
        }
        // Measure must agree with the rendered visual-row count.
        assert_eq!(rows.len(), code_line_row_count(body, 24) as usize);
    }

    #[test]
    fn code_block_measure_matches_rendered_rows() {
        let long = "let value = some_very_long_variable_name_that_exceeds_the_wrap_width_by_a_lot_and_should_wrap = compute();";
        let src = format!(
            "Intro text.\n\n```rust\nfn main() {{\n    let x: i32 = 42;\n    {long}\n    println!(\"{{x}}\");\n}}\n```\n\nAfter."
        );
        let doc = parse_markdown_document(&src);
        let block = render_markdown_block(&doc, 60);
        let rendered = element! { View(width: 60) { #(vec![block]) } }.to_string();
        let rendered_rows = rendered.lines().count();
        let measured = markdown_document_row_count(&doc, 60);
        assert_eq!(
            rendered_rows, measured as usize,
            "measured rows ({measured}) must match rendered rows ({rendered_rows})"
        );
    }

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
        let doc = parse_markdown_document(&format!("```\n{long}\nsecond\n```"));
        let rows = markdown_document_row_count(&doc, 40);
        assert!(rows >= 3, "multi-line code should wrap to multiple rows, got {rows}");
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
        assert!(doc.lines.iter().any(|line| line.kind == MarkdownLineKind::Code));
        assert!(doc.lines.iter().all(|line| !line.code_background));
    }

    #[test]
    fn horizontal_rule_renders_full_width() {
        let doc = parse_markdown_document("Above\n\n---\n\nBelow");
        let width = 32u16;
        let block = render_markdown_block(&doc, width);
        let rendered = element! { View(width: width) { #(vec![block]) } }.to_string();
        assert!(
            rendered.contains(&markdown_horizontal_rule_text(width)),
            "expected full-width rule, got:\n{rendered}"
        );
    }

    #[test]
    fn gfm_table_renders_in_markdown_block() {
        let doc = parse_markdown_document("| Tool | Count |\n| --- | --- |\n| grep | 3 |");
        let block = render_markdown_block(&doc, 50);
        let rendered = element! { View(width: 50) { #(vec![block]) } }.to_string();
        assert!(rendered.contains("grep"));
        assert!(rendered.contains('3'));
    }

    #[test]
    fn single_line_code_block_renders_without_card_background() {
        let doc = parse_markdown_document("```\nhello\n```");
        let block = render_markdown_block(&doc, 40);
        let rendered = element! { View(width: 40) { #(vec![block]) } }.to_string();
        assert!(rendered.contains("hello"));
        assert!(!doc.lines[0].code_background);
    }
}
