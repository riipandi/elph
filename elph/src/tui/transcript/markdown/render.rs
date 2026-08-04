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

/// Check if the raw content has an unclosed fenced codeblock.
/// Uses simple fence counting (odd = unclosed), consistent with `find_stable_boundary`.
fn has_unclosed_fence(content: &str) -> bool {
    let mut count = 0usize;
    let mut pos = 0usize;
    while let Some(rel) = content[pos..].find("```") {
        count += 1;
        pos += rel + 3;
    }
    pos = 0;
    while let Some(rel) = content[pos..].find("~~~") {
        count += 1;
        pos += rel + 3;
    }
    count % 2 == 1
}

/// Render one stable markdown slice from cache.
///
/// When no cached document is available, we render the source as plain text (not markdown),
/// because the stable boundary might have split content in a way that produces incorrect
/// markdown when parsed in isolation. Plain text avoids introducing structural artifacts
/// (e.g., codeblocks split across stable/tail boundaries).
fn render_markdown_part(
    document: Option<&MarkdownDocument>,
    fallback_source: &str,
    fallback_foreground: Color,
) -> MarkdownDocument {
    if let Some(doc) = document {
        return doc.clone();
    }
    // Fallback to plain text — avoids creating partial/incomplete markdown structures
    // when the stable boundary splits content at a non-markdown-safe position.
    plain_text_document(fallback_source, fallback_foreground)
}

/// Build the merged markdown document for an assistant message — the cached stable parts plus
/// the live streaming tail — **without** painting it.
///
/// `render_markdown_buffer` (paint) and the transcript row measurement in `layout.rs`
/// (`assistant_row_count`) both call this so measurement and paint always operate on the exact
/// same document and stay in parity. The previous measure path summed the stable and tail row
/// counts independently and missed the inter-segment gap at the stable↔tail boundary, so the
/// measured height was one row short and the scroll viewport clipped the first line of the
/// following paragraph.
pub(crate) fn build_assistant_markdown_document(
    buffer: &AssistantMarkdownBuffer,
    raw: &str,
    tail_foreground: Color,
) -> MarkdownDocument {
    let mut document = MarkdownDocument::default();
    let mut source_start = 0usize;

    // Process all stable parts as cached markdown (or plain-text fallback before the worker
    // finishes parsing), then merge with the streaming tail.
    for part in &buffer.parts {
        let end = part.source_end.min(raw.len());
        let start = source_start.min(end);
        let Some(slice) = raw.get(start..end) else {
            source_start = end;
            continue;
        };
        let part_doc = render_markdown_part(part.document.as_ref(), slice, tail_foreground);
        document = merge_documents(document, part_doc);
        source_start = end;
    }

    let tail = buffer.tail(raw);
    if !tail.is_empty() {
        // Check if the raw content (stable + tail) has an unclosed fence.
        // When unclosed, we MUST show the entire tail to avoid truncating
        // the codeblock mid-content.
        let has_unclosed = has_unclosed_fence(raw);

        if has_unclosed {
            // In an unclosed codeblock: render the entire tail as markdown
            // to preserve codeblock structure and syntax highlighting.
            let tail_doc = if tail.len() > 12_000 {
                // Safety cap: only last 12K chars for very long streams
                let start = tail.char_indices().rev().nth(11_999).map(|(i, _)| i).unwrap_or(0);
                streaming_tail_document(&tail[start..])
            } else {
                streaming_tail_document(tail)
            };
            document = merge_documents(document, tail_doc);
        } else {
            // Outside codeblock: use capped tail for performance
            const TAIL_PAINT_MAX: usize = 4_000;
            let capped_tail = if tail.len() > TAIL_PAINT_MAX {
                let start = tail
                    .char_indices()
                    .rev()
                    .nth(TAIL_PAINT_MAX.saturating_sub(1))
                    .map(|(i, _)| i)
                    .unwrap_or(0);
                &tail[start..]
            } else {
                tail
            };
            if !capped_tail.is_empty() {
                document = merge_documents(document, streaming_tail_document(capped_tail));
            }
        }
    }

    document
}

/// Render assistant markdown (stable prefix + streaming tail) as one iocraft block.
pub fn render_markdown_buffer(
    buffer: &AssistantMarkdownBuffer,
    raw: &str,
    tail_foreground: Color,
    width: u16,
) -> AnyElement<'static> {
    let width = width.max(1);
    let document = build_assistant_markdown_document(buffer, raw, tail_foreground);
    if document.is_empty() {
        return render_linkified_plain_text(raw, tail_foreground, width);
    }
    render_markdown_block(&document, width)
}
