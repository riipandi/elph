//! Row measurement helpers (shared with ANSI streaming gaps).

use crate::blocks::{CODE_VERTICAL_PADDING, code_content_width, segment_end, segment_gap_after};
use crate::mermaid::mermaid_display_text;
use crate::model::{MarkdownDocument, MarkdownLine, MarkdownLineKind};
use crate::table::format_table_lines;
use crate::theme::MarkdownTheme;
use crate::wrap::{wrap_text_to_lines, wrap_with_hanging_ranges};

fn line_plain_text(line: &MarkdownLine) -> String {
    line.spans.iter().map(|span| span.text.as_str()).collect()
}

fn code_line_row_count(text: &str, inner: u16) -> u16 {
    wrap_with_hanging_ranges(text, inner).len().max(1) as u16
}

fn line_row_count(line: &MarkdownLine, wrap_width: u16, theme: &MarkdownTheme) -> u16 {
    if line.is_blank() {
        return 1;
    }
    if line.kind == MarkdownLineKind::Rule {
        return 1;
    }
    if let Some(source) = &line.mermaid_source {
        let inner = code_content_width(wrap_width);
        return mermaid_display_text(source, inner).lines().count().max(1) as u16;
    }
    if line.code_background {
        let inner = code_content_width(wrap_width);
        return code_line_row_count(&line_plain_text(line), inner).max(1);
    }
    if matches!(line.kind, MarkdownLineKind::Code) {
        return code_line_row_count(&line_plain_text(line), wrap_width).max(1);
    }
    if line.kind == MarkdownLineKind::Table {
        if let Some(table) = &line.table {
            return format_table_lines(table, wrap_width, theme).len().max(1) as u16;
        }
        return 1;
    }
    wrap_text_to_lines(&line_plain_text(line), wrap_width as usize)
        .len()
        .max(1) as u16
}

/// Wrapped row count for a parsed markdown document (includes block gaps).
pub fn markdown_document_row_count(document: &MarkdownDocument, wrap_width: u16, theme: &MarkdownTheme) -> u16 {
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
                let mut block = if lines[index..end].iter().any(|l| l.code_background) {
                    CODE_VERTICAL_PADDING
                } else {
                    0
                };
                for item in &lines[index..end] {
                    block = block.saturating_add(line_row_count(item, wrap_width, theme));
                }
                total = total.saturating_add(block.max(1));
            } else {
                for item in &lines[index..end] {
                    total = total.saturating_add(line_row_count(item, wrap_width, theme));
                }
            }
        } else if line.kind == MarkdownLineKind::ListItem {
            for item in &lines[index..end] {
                total = total.saturating_add(line_row_count(item, wrap_width, theme));
            }
        } else {
            total = total.saturating_add(line_row_count(line, wrap_width, theme));
        }
        total = total.saturating_add(segment_gap_after(lines, index, end));
        index = end;
    }
    total.max(1)
}
