//! pulldown-cmark → [`MarkdownDocument`].

use pulldown_cmark::{Event, Tag, TagEnd};

use super::highlight::highlight_code_block;
use super::linkify::spans_with_links;
use super::model::{MarkdownDocument, MarkdownLine, MarkdownLineKind, MarkdownTable, StyledSpan};
use super::parser_config::offset_events;
use super::theme::MarkdownTheme;

#[derive(Clone, Copy, Default)]
struct SpanStyle {
    color: Option<iocraft::prelude::Color>,
    weight: iocraft::prelude::Weight,
    italic: bool,
}

impl SpanStyle {
    fn apply(self, theme: &MarkdownTheme) -> (iocraft::prelude::Color, iocraft::prelude::Weight, bool) {
        (self.color.unwrap_or(theme.body), self.weight, self.italic)
    }
}

struct ListFrame {
    ordered: bool,
    next_number: u64,
}

struct ParserState<'a> {
    theme: &'a MarkdownTheme,
    lines: Vec<MarkdownLine>,
    style_stack: Vec<SpanStyle>,
    current_spans: Vec<StyledSpan>,
    heading_level: Option<u8>,
    list_stack: Vec<ListFrame>,
    in_blockquote: bool,
    in_code_block: bool,
    in_list_item: bool,
    pending_item_marker: bool,
    code_block_lang: Option<String>,
    code_block_body: String,
    in_table: bool,
    in_table_cell: bool,
    table_rows: Vec<Vec<String>>,
    table_row: Vec<String>,
    table_cell: String,
    current_line_kind: MarkdownLineKind,
    block_has_content: bool,
}

impl<'a> ParserState<'a> {
    fn new(theme: &'a MarkdownTheme) -> Self {
        Self {
            theme,
            lines: Vec::new(),
            style_stack: vec![SpanStyle::default()],
            current_spans: Vec::new(),
            heading_level: None,
            list_stack: Vec::new(),
            in_blockquote: false,
            in_code_block: false,
            in_list_item: false,
            pending_item_marker: false,
            code_block_lang: None,
            code_block_body: String::new(),
            in_table: false,
            in_table_cell: false,
            table_rows: Vec::new(),
            table_row: Vec::new(),
            table_cell: String::new(),
            current_line_kind: MarkdownLineKind::Paragraph,
            block_has_content: false,
        }
    }

    fn heading_prefix(level: u8) -> String {
        format!("{} ", "#".repeat(level as usize))
    }

    fn push_heading_prefix(&mut self, level: u8) {
        self.push_span(StyledSpan {
            text: Self::heading_prefix(level),
            color: self.theme.heading,
            weight: self.theme.heading_weight,
            italic: false,
        });
    }

    fn push_table_line(&mut self) {
        if self.table_rows.is_empty() {
            return;
        }
        self.lines.push(MarkdownLine {
            kind: MarkdownLineKind::Table,
            spans: Vec::new(),
            code_background: false,
            table: Some(MarkdownTable {
                rows: std::mem::take(&mut self.table_rows),
            }),
        });
        self.block_has_content = true;
    }

    fn append_table_cell_text(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        self.table_cell.push_str(text);
    }

    fn current_style(&self) -> SpanStyle {
        *self.style_stack.last().unwrap_or(&SpanStyle::default())
    }

    fn push_style(&mut self, layer: SpanStyle) {
        let base = self.current_style();
        self.style_stack.push(SpanStyle {
            color: layer.color.or(base.color),
            weight: if layer.weight != iocraft::prelude::Weight::Normal {
                layer.weight
            } else {
                base.weight
            },
            italic: base.italic || layer.italic,
        });
    }

    fn pop_style(&mut self) {
        if self.style_stack.len() > 1 {
            self.style_stack.pop();
        }
    }

    fn reset_block(&mut self) {
        self.block_has_content = false;
    }

    fn flush_spans_as(&mut self, kind: MarkdownLineKind) {
        if self.current_spans.is_empty() {
            return;
        }
        self.lines.push(MarkdownLine {
            kind,
            spans: std::mem::take(&mut self.current_spans),
            code_background: false,
            table: None,
        });
        self.block_has_content = true;
    }

    fn flush_spans(&mut self) {
        self.flush_spans_as(self.current_line_kind);
    }

    fn flush_hard_break(&mut self) {
        self.flush_spans_as(MarkdownLineKind::Continuation);
    }

    fn list_indent(&self) -> String {
        "  ".repeat(self.list_stack.len().saturating_sub(1))
    }

    fn list_hanging_indent(&self) -> String {
        format!("{}    ", self.list_indent())
    }

    fn set_block_line_kind(&mut self) {
        self.current_line_kind = if self.in_list_item {
            MarkdownLineKind::ListItem
        } else if self.in_blockquote {
            MarkdownLineKind::Blockquote
        } else {
            MarkdownLineKind::Paragraph
        };
    }

    fn push_list_marker(&mut self, marker: &str) {
        let indent = self.list_indent();
        self.push_span(StyledSpan::plain(format!("{indent}{marker}"), self.theme.list_marker));
        self.pending_item_marker = false;
    }

    fn ensure_list_marker(&mut self) {
        if !self.pending_item_marker {
            return;
        }
        let marker = if let Some(frame) = self.list_stack.last_mut() {
            if frame.ordered {
                let marker = format!("{}. ", frame.next_number);
                frame.next_number += 1;
                marker
            } else {
                "• ".to_string()
            }
        } else {
            "• ".to_string()
        };
        self.push_list_marker(&marker);
    }

    fn push_text(&mut self, text: &str, linkify: bool) {
        if text.is_empty() {
            return;
        }
        self.ensure_list_marker();
        let style = self.current_style();
        let (color, weight, italic) = style.apply(self.theme);
        let segments = if linkify {
            spans_with_links(text, color, weight, italic, self.theme.link)
        } else {
            vec![StyledSpan {
                text: text.to_string(),
                color,
                weight,
                italic,
            }]
        };
        for segment in segments {
            self.push_span(segment);
        }
    }

    fn push_span(&mut self, span: StyledSpan) {
        if let Some(last) = self.current_spans.last_mut()
            && last.color == span.color
            && last.weight == span.weight
            && last.italic == span.italic
        {
            last.text.push_str(&span.text);
            return;
        }
        self.current_spans.push(span);
    }

    fn push_rule(&mut self) {
        self.flush_spans();
        self.reset_block();
        self.lines.push(MarkdownLine {
            kind: MarkdownLineKind::Rule,
            spans: Vec::new(),
            code_background: false,
            table: None,
        });
    }

    fn finish(mut self) -> MarkdownDocument {
        self.flush_spans();
        MarkdownDocument { lines: self.lines }.normalize()
    }
}

/// Parse markdown source off the UI thread (CPU-bound).
pub fn parse_markdown_document(source: &str) -> MarkdownDocument {
    parse_markdown_document_with_theme(source, &MarkdownTheme::default())
}

pub fn parse_markdown_document_with_theme(source: &str, theme: &MarkdownTheme) -> MarkdownDocument {
    if source.is_empty() {
        return MarkdownDocument::default();
    }

    let mut state = ParserState::new(theme);

    for (event, _range) in offset_events(source) {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                state.flush_spans();
                state.reset_block();
                let level = level as u8;
                state.current_line_kind = MarkdownLineKind::Heading(level);
                state.heading_level = Some(level);
                state.push_heading_prefix(level);
                state.push_style(SpanStyle {
                    color: Some(theme.heading),
                    weight: theme.heading_weight,
                    italic: false,
                });
            }
            Event::End(TagEnd::Heading(_)) => {
                state.flush_spans();
                state.pop_style();
                state.heading_level = None;
                state.reset_block();
            }
            Event::Start(Tag::CodeBlock(kind)) => {
                state.flush_spans();
                state.reset_block();
                state.in_code_block = true;
                state.code_block_lang = match kind {
                    pulldown_cmark::CodeBlockKind::Fenced(lang) if !lang.is_empty() => Some(lang.to_string()),
                    _ => None,
                };
                state.code_block_body.clear();
            }
            Event::End(TagEnd::CodeBlock) => {
                state.in_code_block = false;
                let lang = state.code_block_lang.take();
                let body = std::mem::take(&mut state.code_block_body);
                let follows_code_block = state
                    .lines
                    .last()
                    .is_some_and(|line| line.code_background || line.kind == MarkdownLineKind::Code);
                if follows_code_block {
                    state.lines.push(MarkdownLine::blank());
                }
                state.lines.extend(highlight_code_block(lang.as_deref(), &body, theme));
                state.reset_block();
            }
            Event::Start(Tag::Paragraph) => {
                if !state.in_code_block {
                    state.flush_spans();
                    state.reset_block();
                    if state.in_list_item {
                        state.current_line_kind = MarkdownLineKind::ListItem;
                        if state
                            .lines
                            .last()
                            .is_some_and(|line| line.kind == MarkdownLineKind::ListItem)
                        {
                            state.push_text(&state.list_hanging_indent(), false);
                        }
                    } else {
                        state.set_block_line_kind();
                    }
                }
            }
            Event::End(TagEnd::Paragraph) => {
                if !state.in_code_block {
                    state.flush_spans();
                    state.reset_block();
                }
            }
            Event::Start(Tag::List(start)) => {
                state.list_stack.push(ListFrame {
                    ordered: start.is_some(),
                    next_number: start.unwrap_or(1),
                });
            }
            Event::End(TagEnd::List(_)) => {
                state.list_stack.pop();
            }
            Event::Start(Tag::Item) => {
                state.flush_spans();
                state.reset_block();
                state.in_list_item = true;
                state.pending_item_marker = true;
                state.current_line_kind = MarkdownLineKind::ListItem;
            }
            Event::End(TagEnd::Item) => {
                state.flush_spans();
                state.reset_block();
                state.in_list_item = false;
                state.pending_item_marker = false;
            }
            Event::Start(Tag::BlockQuote(_)) => {
                state.flush_spans();
                state.reset_block();
                state.in_blockquote = true;
                state.current_line_kind = MarkdownLineKind::Blockquote;
                state.push_style(SpanStyle {
                    color: Some(theme.blockquote),
                    weight: iocraft::prelude::Weight::Normal,
                    italic: true,
                });
            }
            Event::End(TagEnd::BlockQuote(_)) => {
                state.flush_spans();
                state.in_blockquote = false;
                state.pop_style();
                state.reset_block();
            }
            Event::Start(Tag::Strong) => {
                state.push_style(SpanStyle {
                    color: Some(theme.strong),
                    weight: iocraft::prelude::Weight::Bold,
                    italic: false,
                });
            }
            Event::End(TagEnd::Strong) => state.pop_style(),
            Event::Start(Tag::Emphasis) => {
                state.push_style(SpanStyle {
                    color: Some(theme.emphasis),
                    weight: iocraft::prelude::Weight::Normal,
                    italic: true,
                });
            }
            Event::End(TagEnd::Emphasis) => state.pop_style(),
            Event::Start(Tag::Strikethrough) => {
                state.push_style(SpanStyle {
                    color: Some(theme.blockquote),
                    weight: iocraft::prelude::Weight::Normal,
                    italic: false,
                });
            }
            Event::End(TagEnd::Strikethrough) => state.pop_style(),
            Event::Start(Tag::Superscript) => {
                state.push_style(SpanStyle {
                    color: None,
                    weight: iocraft::prelude::Weight::Normal,
                    italic: false,
                });
            }
            Event::End(TagEnd::Superscript) => state.pop_style(),
            Event::Start(Tag::Subscript) => {
                state.push_style(SpanStyle {
                    color: None,
                    weight: iocraft::prelude::Weight::Normal,
                    italic: false,
                });
            }
            Event::End(TagEnd::Subscript) => state.pop_style(),
            Event::Start(Tag::Link { .. }) => {
                state.push_style(SpanStyle {
                    color: Some(theme.link),
                    weight: iocraft::prelude::Weight::Normal,
                    italic: false,
                });
            }
            Event::End(TagEnd::Link) => state.pop_style(),
            Event::Start(Tag::Image { dest_url, title, .. }) => {
                let label = if title.is_empty() {
                    format!("[image]({dest_url})")
                } else {
                    format!("[{title}]({dest_url})")
                };
                state.push_text(&label, true);
            }
            Event::End(TagEnd::Image) => {}
            Event::Start(Tag::Table(_)) => {
                state.flush_spans();
                state.reset_block();
                state.in_table = true;
                state.table_rows.clear();
                state.table_row.clear();
                state.table_cell.clear();
            }
            Event::End(TagEnd::Table) => {
                state.push_table_line();
                state.in_table = false;
                state.in_table_cell = false;
                state.reset_block();
            }
            Event::Start(Tag::TableHead) | Event::Start(Tag::TableRow) => {
                state.table_row.clear();
            }
            Event::End(TagEnd::TableHead) | Event::End(TagEnd::TableRow) => {
                if !state.table_row.is_empty() {
                    state.table_rows.push(std::mem::take(&mut state.table_row));
                }
            }
            Event::Start(Tag::TableCell) => {
                state.table_cell.clear();
                state.in_table_cell = true;
            }
            Event::End(TagEnd::TableCell) => {
                state.table_row.push(std::mem::take(&mut state.table_cell));
                state.in_table_cell = false;
            }
            Event::Start(Tag::HtmlBlock) | Event::Start(Tag::MetadataBlock(_)) => {
                state.flush_spans();
                state.reset_block();
            }
            Event::End(TagEnd::HtmlBlock) | Event::End(TagEnd::MetadataBlock(_)) => {
                state.flush_spans();
                state.reset_block();
            }
            Event::Start(Tag::FootnoteDefinition(_)) | Event::Start(Tag::DefinitionList) => {
                state.flush_spans();
                state.reset_block();
            }
            Event::End(TagEnd::FootnoteDefinition) | Event::End(TagEnd::DefinitionList) => {
                state.flush_spans();
                state.reset_block();
            }
            Event::Start(Tag::DefinitionListTitle) | Event::Start(Tag::DefinitionListDefinition) => {
                state.flush_spans();
                state.reset_block();
                state.set_block_line_kind();
            }
            Event::End(TagEnd::DefinitionListTitle) | Event::End(TagEnd::DefinitionListDefinition) => {
                state.flush_spans();
                state.reset_block();
            }
            Event::Code(text) => {
                if state.in_table_cell {
                    state.append_table_cell_text(&text);
                } else if state.in_code_block {
                    state.code_block_body.push_str(&text);
                } else {
                    state.push_style(SpanStyle {
                        color: Some(theme.inline_code),
                        weight: iocraft::prelude::Weight::Normal,
                        italic: false,
                    });
                    state.push_text(&text, false);
                    state.pop_style();
                }
            }
            Event::Text(text) => {
                if state.in_table_cell {
                    state.append_table_cell_text(&text);
                } else if state.in_code_block {
                    state.code_block_body.push_str(&text);
                } else {
                    state.push_text(&text, true);
                }
            }
            Event::SoftBreak => {
                if state.in_table_cell {
                    state.append_table_cell_text(" ");
                } else {
                    state.push_text(" ", true);
                }
            }
            Event::HardBreak => state.flush_hard_break(),
            Event::Rule => state.push_rule(),
            Event::InlineHtml(html) => {
                if !state.in_code_block {
                    state.push_text(html_escape::decode_html_entities(&html).as_ref(), false);
                }
            }
            Event::InlineMath(text) | Event::DisplayMath(text) => {
                if !state.in_code_block {
                    state.push_text(&text, false);
                }
            }
            Event::FootnoteReference(label) => {
                state.push_text(&format!("[^{label}]"), false);
            }
            Event::TaskListMarker(checked) => {
                let marker = if checked { "[x] " } else { "[ ] " };
                state.push_list_marker(marker);
            }
            _ => {}
        }
    }

    let mut doc = state.finish();
    if doc.lines.is_empty() {
        doc.lines.push(MarkdownLine {
            kind: MarkdownLineKind::Paragraph,
            spans: vec![StyledSpan::plain("", theme.body)],
            code_background: false,
            table: None,
        });
    }
    doc
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::markdown::markdown_document_row_count;

    fn line_texts(doc: &MarkdownDocument) -> Vec<(MarkdownLineKind, String)> {
        doc.lines
            .iter()
            .map(|line| {
                let text: String = line.spans.iter().map(|s| s.text.as_str()).collect();
                (line.kind, text)
            })
            .collect()
    }

    #[test]
    fn two_paragraphs_get_single_gap_not_double_blank() {
        let doc = parse_markdown_document("One\n\nTwo");
        assert!(
            !doc.lines.iter().any(|line| line.is_blank()),
            "unexpected blank lines: {:?}",
            line_texts(&doc)
        );
        assert_eq!(doc.lines.len(), 2);
        let rows = markdown_document_row_count(&doc, 40);
        assert!(rows <= 4, "too many rows for two short paragraphs: {rows}");
    }

    #[test]
    fn list_items_stay_list_items_inside_paragraph() {
        let doc = parse_markdown_document("- alpha\n- beta");
        assert!(
            doc.lines
                .iter()
                .all(|line| matches!(line.kind, MarkdownLineKind::ListItem))
        );
    }

    #[test]
    fn ordered_list_uses_numbers() {
        let doc = parse_markdown_document("1. first\n2. second");
        let first: String = doc.lines[0].spans.iter().map(|s| s.text.as_str()).collect();
        assert!(first.starts_with("1. "), "first line: {first}");
    }

    #[test]
    fn task_list_renders_checkbox_before_text() {
        let doc = parse_markdown_document("- [x] done\n- [ ] todo");
        let done: String = doc.lines[0].spans.iter().map(|s| s.text.as_str()).collect();
        assert!(done.contains("[x]"), "done line: {done}");
        assert!(!done.starts_with('•'), "task list should not use bullet: {done}");
    }

    #[test]
    fn list_followed_by_paragraph_has_gap_row() {
        let doc = parse_markdown_document("- item\n\nnext");
        assert_eq!(doc.lines.len(), 2);
        let rows = markdown_document_row_count(&doc, 40);
        assert!(rows >= 3, "expected gap after list, got {rows}");
    }

    #[test]
    fn hard_break_uses_continuation_without_extra_gap() {
        let doc = parse_markdown_document("line one  \nline two");
        let kinds: Vec<_> = doc.lines.iter().map(|l| l.kind).collect();
        assert!(kinds.contains(&MarkdownLineKind::Continuation));
    }

    #[test]
    fn blockquote_preserves_kind() {
        let doc = parse_markdown_document("> quoted text");
        assert!(
            doc.lines
                .iter()
                .any(|line| matches!(line.kind, MarkdownLineKind::Blockquote))
        );
    }

    #[test]
    fn triple_newline_collapses_extra_blank() {
        let doc = parse_markdown_document("a\n\n\nb");
        let blank_count = doc.lines.iter().filter(|line| line.is_blank()).count();
        assert!(blank_count <= 1, "blanks: {:?}", line_texts(&doc));
    }

    #[test]
    fn heading_keeps_markdown_prefix_with_highlight_color() {
        let doc = parse_markdown_document("# Title one\n## Title two");
        let first: String = doc.lines[0].spans.iter().map(|s| s.text.as_str()).collect();
        let second: String = doc.lines[1].spans.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(first, "# Title one");
        assert_eq!(second, "## Title two");
        assert!(
            doc.lines[0]
                .spans
                .iter()
                .all(|s| s.color == MarkdownTheme::default().heading)
        );
    }

    #[test]
    fn list_with_leading_emoji_preserves_marker_and_text() {
        let doc = parse_markdown_document("- ✅ Done\n- 🚀 Launch");
        assert_eq!(doc.lines.len(), 2);
        let done: String = doc.lines[0].spans.iter().map(|s| s.text.as_str()).collect();
        let launch: String = doc.lines[1].spans.iter().map(|s| s.text.as_str()).collect();
        assert!(done.starts_with("• "), "done line: {done}");
        assert!(done.contains("✅ Done"), "done line: {done}");
        assert!(launch.starts_with("• "), "launch line: {launch}");
        assert!(launch.contains("🚀 Launch"), "launch line: {launch}");
    }

    #[test]
    fn thematic_break_parses_as_rule_line() {
        let doc = parse_markdown_document("Above\n\n---\n\nBelow");
        assert!(
            doc.lines.iter().any(|line| line.kind == MarkdownLineKind::Rule),
            "lines: {:?}",
            doc.lines.iter().map(|l| l.kind).collect::<Vec<_>>()
        );
        let rule = doc
            .lines
            .iter()
            .find(|line| line.kind == MarkdownLineKind::Rule)
            .expect("rule");
        assert!(rule.spans.is_empty());
        assert!(!rule.is_blank());
    }

    #[test]
    fn gfm_table_parses_into_table_block() {
        let doc = parse_markdown_document("| Name | Status |\n| --- | --- |\n| Ada | ✅ |");
        assert!(
            doc.lines.iter().any(|line| line.kind == MarkdownLineKind::Table),
            "lines: {:?}",
            line_texts(&doc)
        );
        let table = doc
            .lines
            .iter()
            .find(|line| line.kind == MarkdownLineKind::Table)
            .expect("table line");
        let matrix = table.table.as_ref().expect("table matrix");
        assert_eq!(matrix.rows.len(), 2);
        assert_eq!(matrix.rows[0], vec!["Name", "Status"]);
        assert_eq!(matrix.rows[1], vec!["Ada", "✅"]);
    }
}
