//! Row measurement for cached markdown documents.

use crate::text_input_layout::WrappedTextLayout;
use crate::wrapped_transcript_row_count;

use super::blocks::CODE_VERTICAL_PADDING;
use super::blocks::{code_content_width, segment_end, segment_gap_after};
use super::model::{MarkdownDocument, MarkdownLine, MarkdownLineKind};
use super::table::markdown_table_row_count;

fn line_plain_text(line: &MarkdownLine) -> String {
    line.spans.iter().map(|span| span.text.as_str()).collect()
}

fn wrapped_line_row_count(text: &str, wrap_width: u16) -> u16 {
    if text.is_empty() {
        return 0;
    }
    WrappedTextLayout::new_for_overlay_editor(text, wrap_width)
        .row_count()
        .max(1)
}

fn line_row_count(line: &MarkdownLine, wrap_width: u16) -> u16 {
    if line.is_blank() {
        return 1;
    }
    if line.kind == MarkdownLineKind::Rule {
        return 1;
    }
    if line.code_background {
        let inner = code_content_width(wrap_width);
        return wrapped_line_row_count(&line_plain_text(line), inner).max(1);
    }
    if matches!(line.kind, MarkdownLineKind::Code) {
        return wrapped_line_row_count(&line_plain_text(line), wrap_width).max(1);
    }
    wrapped_line_row_count(&line_plain_text(line), wrap_width).max(1)
}

fn code_block_row_count(lines: &[MarkdownLine], wrap_width: u16) -> u16 {
    let mut total = if lines.iter().any(|line| line.code_background) {
        CODE_VERTICAL_PADDING
    } else {
        0
    };
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
            if lines[index..end].iter().any(|item| item.code_background) {
                total = total.saturating_add(code_block_row_count(&lines[index..end], wrap_width));
            } else {
                for item in &lines[index..end] {
                    total = total.saturating_add(line_row_count(item, wrap_width));
                }
            }
        } else if line.kind == MarkdownLineKind::ListItem {
            for item in &lines[index..end] {
                total = total.saturating_add(line_row_count(item, wrap_width));
            }
        } else if line.kind == MarkdownLineKind::Table {
            if let Some(table) = &line.table {
                total = total.saturating_add(markdown_table_row_count(table, wrap_width));
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
        let doc = parse_markdown_document(&format!("```\n{long}\nmore\n```"));
        let rows = markdown_document_row_count(&doc, 40);
        assert!(rows >= 3, "expected wrapped multi-line code rows, got {rows}");
    }

    #[test]
    fn multi_line_code_block_row_count_includes_vertical_inset() {
        let doc = parse_markdown_document("```\nline one\nline two\n```");
        let rows = markdown_document_row_count(&doc, 40);
        assert_eq!(rows, 4, "two code lines plus top/bottom inset");
    }

    #[test]
    fn single_line_code_block_wraps_at_full_width_without_card_padding() {
        let long = "x".repeat(80);
        let doc = parse_markdown_document(&format!("```\n{long}\n```"));
        let rows = markdown_document_row_count(&doc, 40);
        assert_eq!(rows, 2, "single-line code should wrap at full width without card inset");
        assert!(doc.lines.iter().all(|line| !line.code_background));
    }

    #[test]
    fn emoji_list_row_count_uses_display_width() {
        let doc = parse_markdown_document("- ✅ Done");
        let rows = markdown_document_row_count(&doc, 8);
        assert!(rows >= 2, "wide emoji should wrap before byte-count heuristics, got {rows}");
    }

    #[test]
    fn gfm_table_row_count_uses_flex_layout() {
        let doc = parse_markdown_document("| Name | Status |\n| --- | --- |\n| Ada | ✅ |");
        let rows = markdown_document_row_count(&doc, 40);
        assert!(rows >= 3, "bordered table should consume multiple rows, got {rows}");
    }

    #[test]
    fn code_blocks_use_same_gap_as_paragraphs() {
        use super::super::blocks::BLOCK_GAP_ROWS;

        let para_code = markdown_document_row_count(&parse_markdown_document("Text\n\n```\ncode\n```"), 40);
        let code_para = markdown_document_row_count(&parse_markdown_document("```\ncode\n```\n\nText"), 40);
        let code_code = markdown_document_row_count(&parse_markdown_document("```\na\n```\n```\nb\n```"), 40);

        let single_code = markdown_document_row_count(&parse_markdown_document("```\ncode\n```"), 40);
        let single_para = markdown_document_row_count(&parse_markdown_document("Text"), 40);

        assert_eq!(
            para_code,
            single_para + BLOCK_GAP_ROWS + single_code,
            "paragraph → code should add exactly one gap row"
        );
        assert_eq!(
            code_para,
            single_code + BLOCK_GAP_ROWS + single_para,
            "code → paragraph should add exactly one gap row"
        );
        assert_eq!(
            code_code,
            markdown_document_row_count(&parse_markdown_document("```\na\n```"), 40)
                + BLOCK_GAP_ROWS
                + markdown_document_row_count(&parse_markdown_document("```\nb\n```"), 40),
            "adjacent code blocks should add exactly one gap row"
        );
    }
}
