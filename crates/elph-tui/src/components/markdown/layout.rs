//! Row measurement for cached markdown documents.

use crate::wrapped_transcript_row_count;
use textwrap::wrap;

use super::blocks::{CODE_VERTICAL_PADDING, code_content_width, segment_end, segment_gap_after};
use super::model::{MarkdownDocument, MarkdownLine, MarkdownLineKind};

fn line_plain_text(line: &MarkdownLine) -> String {
    line.spans.iter().map(|span| span.text.as_str()).collect()
}

fn wrapped_line_row_count(text: &str, wrap_width: u16) -> u16 {
    if text.is_empty() {
        return 0;
    }
    let width = usize::from(wrap_width.max(1));
    wrap(text, width).len().max(1) as u16
}

fn line_row_count(line: &MarkdownLine, wrap_width: u16) -> u16 {
    if line.is_blank() {
        return 1;
    }
    if line.code_background || matches!(line.kind, MarkdownLineKind::Code) {
        let inner = code_content_width(wrap_width);
        return wrapped_line_row_count(&line_plain_text(line), inner).max(1);
    }
    wrapped_line_row_count(&line_plain_text(line), wrap_width).max(1)
}

fn code_block_row_count(lines: &[MarkdownLine], wrap_width: u16) -> u16 {
    let mut total = CODE_VERTICAL_PADDING;
    for line in lines {
        total = total.saturating_add(line_row_count(line, wrap_width));
    }
    total.max(1)
}

/// Wrapped row count for a parsed markdown document (includes block gaps).
pub fn markdown_document_row_count(document: &MarkdownDocument, wrap_width: u16) -> u16 {
    let mut total = 0u16;
    let lines = &document.lines;
    let mut index = 0usize;
    while index < lines.len() {
        let end = segment_end(lines, index);
        let line = &lines[index];
        if line.is_blank() {
            total = total.saturating_add(1);
        } else if line.code_background || line.kind == MarkdownLineKind::Code {
            total = total.saturating_add(code_block_row_count(&lines[index..end], wrap_width));
        } else if line.kind == MarkdownLineKind::ListItem {
            for item in &lines[index..end] {
                total = total.saturating_add(line_row_count(item, wrap_width));
            }
        } else {
            total = total.saturating_add(line_row_count(line, wrap_width));
        }
        total = total.saturating_add(segment_gap_after(lines, index, end));
        index = end;
    }
    total.max(1)
}

/// Fallback row count from raw markdown source.
pub fn markdown_source_row_count(source: &str, wrap_width: u16) -> u16 {
    wrapped_transcript_row_count(source, wrap_width)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::markdown::parse_markdown_document;

    #[test]
    fn paragraph_gaps_increase_row_count() {
        let doc = parse_markdown_document("One\n\nTwo");
        let tight = markdown_document_row_count(&doc, 40);
        let single = markdown_document_row_count(&parse_markdown_document("OneTwo"), 40);
        assert!(tight >= single);
    }

    #[test]
    fn blank_line_counts_as_one_row() {
        let mut doc = MarkdownDocument::default();
        doc.lines.push(MarkdownLine::blank());
        assert_eq!(markdown_document_row_count(&doc, 40), 1);
    }

    #[test]
    fn code_block_row_count_includes_padding_and_wrap() {
        let long = "x".repeat(80);
        let doc = parse_markdown_document(&format!("```\n{long}\n```"));
        let rows = markdown_document_row_count(&doc, 40);
        assert!(rows >= 4, "expected wrapped code rows with padding, got {rows}");
    }
}
