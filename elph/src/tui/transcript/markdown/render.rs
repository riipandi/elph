//! Paint cached markdown documents into transcript cards.

use elph_tui::MarkdownDocument;
use elph_tui::{plain_text_document, render_linkified_plain_text, render_markdown_block, streaming_tail_document};
use iocraft::prelude::*;

use super::buffer::AssistantMarkdownBuffer;

fn merge_documents(mut base: MarkdownDocument, extension: MarkdownDocument) -> MarkdownDocument {
    if !extension.lines.is_empty() {
        base.lines.extend(extension.lines);
    }
    base.normalize()
}

/// Render one stable markdown slice from cache (falls back to linkified plain text).
fn render_markdown_part(
    document: Option<&MarkdownDocument>,
    fallback_source: &str,
    fallback_foreground: Color,
    _width: u16,
) -> MarkdownDocument {
    if let Some(doc) = document {
        return doc.clone();
    }
    plain_text_document(fallback_source, fallback_foreground)
}

/// Render assistant markdown (stable prefix + streaming tail) as one iocraft block.
pub fn render_markdown_buffer(
    buffer: &AssistantMarkdownBuffer,
    raw: &str,
    tail_foreground: Color,
    width: u16,
) -> AnyElement<'static> {
    let width = width.max(1);
    let mut document = MarkdownDocument::default();
    let mut source_start = 0usize;
    for part in &buffer.parts {
        let end = part.source_end.min(raw.len());
        let start = source_start.min(end);
        // Char-safe: skip invalid ranges instead of panicking the TUI.
        let Some(slice) = raw.get(start..end) else {
            source_start = end;
            continue;
        };
        let part_doc = render_markdown_part(part.document.as_ref(), slice, tail_foreground, width);
        document = merge_documents(document, part_doc);
        source_start = end;
    }
    let mut tail = buffer.tail(raw);
    // Bound live paint cost: only the recent streaming tail is re-parsed each frame.
    const TAIL_PAINT_MAX: usize = 4_000;
    if tail.len() > TAIL_PAINT_MAX {
        let start = tail
            .char_indices()
            .rev()
            .nth(TAIL_PAINT_MAX.saturating_sub(1))
            .map(|(i, _)| i)
            .unwrap_or(0);
        tail = &tail[start..];
    }
    if !tail.is_empty() {
        document = merge_documents(document, streaming_tail_document(tail));
    }
    if document.is_empty() {
        return render_linkified_plain_text(raw, tail_foreground, width);
    }
    render_markdown_block(&document, width)
}
