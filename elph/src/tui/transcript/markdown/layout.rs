//! Scroll row measurement for markdown assistant cards.

use elph_tui::{markdown_document_row_count, wrapped_text_row_count};
use iocraft::prelude::Color;

use super::buffer::AssistantMarkdownBuffer;
use super::render::build_assistant_markdown_document;

pub fn markdown_part_row_count(source: &str, wrap_width: u16) -> u16 {
    wrapped_text_row_count(source, wrap_width as usize).min(u16::MAX as usize) as u16
}

pub fn assistant_row_count(content: &str, markdown: Option<&AssistantMarkdownBuffer>, wrap_width: u16) -> u16 {
    let Some(md) = markdown else {
        return wrapped_text_row_count(content, wrap_width as usize).min(u16::MAX as usize) as u16;
    };
    // Build the exact same merged document the renderer paints, then measure it. Summing the
    // stable and tail row counts independently missed the inter-segment gap at the stable↔tail
    // boundary, so the measured height ran one row short and the scroll viewport clipped the
    // first line of the following paragraph.
    let document = build_assistant_markdown_document(md, content, Color::Reset);
    if document.is_empty() {
        return 1;
    }
    markdown_document_row_count(&document, wrap_width)
}

#[cfg(test)]
mod tests {
    use super::super::buffer::{AssistantMarkdownBuffer, RenderedPart, stable_source_hash};
    use super::super::render::render_markdown_buffer;
    use super::assistant_row_count;
    use elph_tui::{MarkdownDocument, parse_markdown_document};
    use iocraft::prelude::*;

    /// Paint the assistant markdown buffer and count the iocraft rows it occupies.
    fn painted_rows(buffer: &AssistantMarkdownBuffer, content: &str, width: u16) -> usize {
        let el = render_markdown_buffer(buffer, content, Color::Reset, width);
        element! { View(width: width) { #(vec![el]) } }
            .to_string()
            .lines()
            .count()
    }

    /// Simulate a streaming buffer whose stable prefix (up to the first blank line) has been
    /// parsed and cached, with `content[stable_end..]` still in the live tail.
    fn make_buffer(content: &str, width: u16, cached: bool) -> AssistantMarkdownBuffer {
        let stable_end = content.find("\n\n").map(|i| i + 2).unwrap_or(content.len());
        let stable_src = &content[..stable_end];
        let document: Option<MarkdownDocument> = if cached {
            Some(parse_markdown_document(stable_src))
        } else {
            None
        };
        let hash = stable_source_hash(stable_src);
        AssistantMarkdownBuffer {
            stable_end,
            parts: vec![RenderedPart {
                source_end: stable_end,
                source_hash: hash,
                row_count: 1,
                document,
            }],
            wrap_width: width,
            stream_complete: false,
        }
    }

    #[test]
    fn measure_matches_paint_across_stable_tail_boundary() {
        // The stable↔tail boundary sits between two paragraphs; measurement must count the
        // inter-segment gap so the scroll viewport does not clip the first line of the tail.
        let content = "Para1.\n\nPara2.\n\nPara3.";
        for width in [36u16, 40, 60, 80, 120] {
            let buf = make_buffer(content, width, true);
            let measured = assistant_row_count(content, Some(&buf), width);
            let painted = painted_rows(&buf, content, width);
            assert_eq!(
                measured, painted as u16,
                "width {width}: measured {measured} != painted {painted}"
            );
        }
    }

    #[test]
    fn measure_matches_paint_with_code_block_in_tail() {
        let content = "Intro.\n\n```rust\nfn main() {\n    let x = 1;\n}\n```\n\nAfter.";
        for width in [40u16, 60, 80] {
            let buf = make_buffer(content, width, true);
            let measured = assistant_row_count(content, Some(&buf), width);
            let painted = painted_rows(&buf, content, width);
            assert_eq!(
                measured, painted as u16,
                "width {width}: measured {measured} != painted {painted}"
            );
        }
    }

    #[test]
    fn measure_matches_paint_with_uncached_stable_fallback() {
        // Before the worker caches the stable doc, the stable part renders as plain text; the
        // measurement path must mirror that fallback exactly.
        let content = "Here is the plan.\n\n- First, understand the cause.\n- Second, add a guard.\n\nDone.";
        for width in [44u16, 60, 80] {
            let buf = make_buffer(content, width, false);
            let measured = assistant_row_count(content, Some(&buf), width);
            let painted = painted_rows(&buf, content, width);
            assert_eq!(
                measured, painted as u16,
                "width {width}: measured {measured} != painted {painted}"
            );
        }
    }

    #[test]
    fn measure_matches_paint_for_completed_stream() {
        // Once the stream completes the whole reply is the stable prefix and the tail is empty.
        let content = "Para1.\n\nPara2.\n\nPara3.";
        let full_doc = parse_markdown_document(content);
        let buf = AssistantMarkdownBuffer {
            stable_end: content.len(),
            parts: vec![RenderedPart {
                source_end: content.len(),
                source_hash: stable_source_hash(content),
                row_count: 1,
                document: Some(full_doc),
            }],
            wrap_width: 80,
            stream_complete: true,
        };
        let measured = assistant_row_count(content, Some(&buf), 80);
        let painted = painted_rows(&buf, content, 80);
        assert_eq!(measured, painted as u16, "measured {measured} != painted {painted}");
    }
}
