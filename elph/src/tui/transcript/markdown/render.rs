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
    _width: u16,
) -> MarkdownDocument {
    if let Some(doc) = document {
        return doc.clone();
    }
    // Fallback to plain text — avoids creating partial/incomplete markdown structures
    // when the stable boundary splits content at a non-markdown-safe position.
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

    // Process all stable parts as plain text (prevents structural artifacts
    // from partial markdown parsing at slice boundaries)
    for part in &buffer.parts {
        let end = part.source_end.min(raw.len());
        let start = source_start.min(end);
        let Some(slice) = raw.get(start..end) else {
            source_start = end;
            continue;
        };
        let part_doc = render_markdown_part(part.document.as_ref(), slice, tail_foreground, width);
        document = merge_documents(document, part_doc);
        source_start = end;
    }

    // Handle streaming tail with codeblock-preservation logic
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

    if document.is_empty() {
        return render_linkified_plain_text(raw, tail_foreground, width);
    }

    render_markdown_block(&document, width)
}
