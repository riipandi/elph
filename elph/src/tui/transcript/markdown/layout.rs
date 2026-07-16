//! Scroll row measurement for markdown assistant cards.

use elph_tui::{markdown_document_row_count, streaming_tail_document, wrapped_transcript_row_count};

use super::buffer::AssistantMarkdownBuffer;

pub fn markdown_part_row_count(source: &str, wrap_width: u16) -> u16 {
    wrapped_transcript_row_count(source, wrap_width)
}

pub fn assistant_row_count(content: &str, markdown: Option<&AssistantMarkdownBuffer>, wrap_width: u16) -> u16 {
    let Some(md) = markdown else {
        return wrapped_transcript_row_count(content, wrap_width);
    };
    let stable_rows: u16 = md
        .parts
        .iter()
        .map(|part| {
            part.document
                .as_ref()
                .map(|doc| markdown_document_row_count(doc, wrap_width))
                .unwrap_or(part.row_count)
        })
        .sum();
    let tail = md.tail(content);
    if tail.is_empty() {
        return stable_rows.max(1);
    }
    let tail_doc = streaming_tail_document(tail);
    stable_rows.saturating_add(markdown_document_row_count(&tail_doc, wrap_width))
}
