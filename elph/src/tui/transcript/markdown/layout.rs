//! Scroll row measurement for markdown assistant cards.

use elph_tui::{markdown_document_row_count, streaming_tail_document, wrapped_transcript_row_count};

use super::buffer::AssistantMarkdownBuffer;

pub fn markdown_part_row_count(source: &str, wrap_width: u16) -> u16 {
    wrapped_transcript_row_count(source, wrap_width)
}

/// Prefer plain wrap for large streaming tails — full markdown parse of the live tail is too costly.
const STREAMING_TAIL_MARKDOWN_MAX_CHARS: usize = 800;
/// Hard cap on measured streaming tail so wrap work stays bounded.
const STREAMING_TAIL_MEASURE_MAX_CHARS: usize = 4_000;

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
    // Bound measure input (tail of stream only) — older tokens live in stable_rows.
    let tail = if tail.len() > STREAMING_TAIL_MEASURE_MAX_CHARS {
        let start = tail
            .char_indices()
            .rev()
            .nth(STREAMING_TAIL_MEASURE_MAX_CHARS.saturating_sub(1))
            .map(|(i, _)| i)
            .unwrap_or(0);
        &tail[start..]
    } else {
        tail
    };
    // Cheap path for large live tails — avoid re-parsing markdown every layout.
    if tail.len() > STREAMING_TAIL_MARKDOWN_MAX_CHARS {
        return stable_rows.saturating_add(wrapped_transcript_row_count(tail, wrap_width));
    }
    let tail_doc = streaming_tail_document(tail);
    stable_rows.saturating_add(markdown_document_row_count(&tail_doc, wrap_width))
}
