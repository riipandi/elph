//! Scroll row measurement for markdown assistant cards.

use elph_tui::{markdown_document_row_count, wrapped_text_row_count};
use iocraft::prelude::Color;

use super::buffer::AssistantMarkdownBuffer;

pub fn markdown_part_row_count(source: &str, wrap_width: u16) -> u16 {
    wrapped_text_row_count(source, wrap_width as usize).min(u16::MAX as usize) as u16
}

pub fn assistant_row_count(
    content: &str,
    markdown: Option<&std::sync::Arc<AssistantMarkdownBuffer>>,
    wrap_width: u16,
) -> u16 {
    let Some(md) = markdown else {
        return wrapped_text_row_count(content, wrap_width as usize).min(u16::MAX as usize) as u16;
    };
    // Fast path: a completed message whose stable prefix covers the whole content and whose
    // cached document is present can be measured directly from the cache — no need to clone
    // the document through `built_document`. This avoids the biggest per-frame allocation
    // in the layout path and covers the vast majority of messages on screen in a long session.
    if md.stream_complete
        && md.stable_end >= content.len()
        && md.wrap_width == wrap_width
        && md.parts.first().is_some_and(|p| p.document.is_some())
    {
        return markdown_document_row_count(md.parts[0].document.as_ref().expect("checked above"), wrap_width);
    }
    // Build the exact same merged document the renderer paints, then measure it. The buffer's
    // built_doc_cache ensures layout and paint share the same parsed document within a frame.
    let document = md.built_document(content, Color::Reset);
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
        let el = render_markdown_buffer(buffer, content, Color::Reset, width)
            .expect("paintable content must produce a block");
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
        let mut buf = AssistantMarkdownBuffer::new();
        buf.stable_end = stable_end;
        buf.parts = vec![RenderedPart {
            source_end: stable_end,
            source_hash: hash,
            row_count: 1,
            document,
        }];
        buf.wrap_width = width;
        buf.stream_complete = false;
        buf
    }

    #[test]
    fn measure_matches_paint_across_stable_tail_boundary() {
        // The stable↔tail boundary sits between two paragraphs; measurement must count the
        // inter-segment gap so the scroll viewport does not clip the first line of the tail.
        let content = "Para1.\n\nPara2.\n\nPara3.";
        for width in [36u16, 40, 60, 80, 120] {
            let buf = std::sync::Arc::new(make_buffer(content, width, true));
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
            let buf = std::sync::Arc::new(make_buffer(content, width, true));
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
            let buf = std::sync::Arc::new(make_buffer(content, width, false));
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
        let mut buf = AssistantMarkdownBuffer::new();
        buf.stable_end = content.len();
        buf.parts = vec![RenderedPart {
            source_end: content.len(),
            source_hash: stable_source_hash(content),
            row_count: 1,
            document: Some(full_doc),
        }];
        buf.wrap_width = 80;
        buf.stream_complete = true;
        let buf = std::sync::Arc::new(buf);
        let measured = assistant_row_count(content, Some(&buf), 80);
        let painted = painted_rows(&buf, content, 80);
        assert_eq!(measured, painted as u16, "measured {measured} != painted {painted}");
    }

    /// Full streaming scenario: assistant content with paragraphs, table, and mermaid diagram.
    /// Measure must equal paint at every stage so the scroll viewport never clips content.
    #[test]
    fn mixed_content_measure_matches_paint_across_stream() {
        use crate::tui::transcript::markdown::worker::partition_assistant_markdown;
        use crate::tui::transcript::types::TranscriptMessage;

        // Chunks arrive like an LLM stream — paragraph, table, mermaid, closing paragraph.
        let chunks = vec![
            "# Plan\n\n",
            "First paragraph introducing the plan.\n\n",
            "| Step | Tool |\n| --- | --- |\n| 1 | read_file |\n| 2 | edit_file |\n",
            "\nThen a diagram:\n\n",
            "```mermaid\ngraph TD\n    A[Start] --> B[End]\n```\n",
            "\nFinal conclusion paragraph.\n",
        ];

        let mut messages = vec![TranscriptMessage::assistant_markdown(String::new())];
        let mut raw = String::new();

        for chunk in &chunks {
            raw.push_str(chunk);
            messages[0].content = raw.clone();
            partition_assistant_markdown(&mut messages, 80);

            // Measure the rendered height.
            let buffer = messages[0].markdown.as_ref().expect("buffer");
            let measured = assistant_row_count(&raw, Some(buffer), 80);
            // Paint and count rows.
            let painted = painted_rows(buffer, &raw, 80) as u16;
            assert_eq!(
                measured,
                painted,
                "stream phase {:?}: measured {measured} != painted {painted}",
                truncate_debug(&raw)
            );
        }
    }

    /// Mirror `agent_bridge::finalize_turn` exactly: trim trailing whitespace on the content
    /// and mark the buffered stream complete — but the worker refresh has NOT run yet, so the
    /// buffer's `stable_end` may exceed the trimmed content length. Measure must still equal
    /// paint (no drift) so scrolling up after completion never shows clipped content.
    #[test]
    fn finalized_stream_measure_matches_paint_with_pending_worker() {
        use crate::tui::transcript::types::TranscriptMessage;

        // Long enough to wrap, with trailing whitespace that finalize_turn trims.
        let base = "## Plan\n\nParagraph that is long enough to wrap across the width several times over.\n\n| Tool | Note |\n| --- | --- |\n| read_file | reads |\n| write_file | writes |\n\nDone.\n";
        let mut msg = TranscriptMessage::assistant_markdown(format!("{base}   \n\n"));
        let content_trimmed = msg.content.trim_end().to_string();

        // Simulate finalize_turn.
        msg.content = content_trimmed.clone();
        if let Some(md) = msg.markdown.as_mut() {
            std::sync::Arc::make_mut(md).mark_stream_complete();
            // NOTE: no refresh_stable here — the worker tick runs 120ms later, so the buffer
            // still holds the PRE-trim stable_end (which can exceed the trimmed length).
        }

        let md = msg.markdown.as_ref().expect("buffer");
        for width in [44u16, 60, 80] {
            let measured = assistant_row_count(&content_trimmed, Some(md), width);
            let painted = painted_rows(md, &content_trimmed, width) as u16;
            assert_eq!(
                measured, painted,
                "width {width}: after finalize, measured {measured} != painted {painted}"
            );
        }

        // After the worker catches up (refresh_stable), parity must hold too.
        if let Some(md) = msg.markdown.as_mut() {
            std::sync::Arc::make_mut(md).refresh_stable(&msg.content, 80);
        }
        let md = msg.markdown.as_ref().expect("buffer");
        let measured = assistant_row_count(&content_trimmed, Some(md), 80);
        let painted = painted_rows(md, &content_trimmed, 80) as u16;
        assert_eq!(measured, painted, "after refresh: measured {measured} != painted {painted}");
    }

    fn truncate_debug(text: &str) -> String {
        let mut out = String::new();
        for c in text.chars().take(60) {
            out.push(c);
        }
        if text.chars().count() > 60 {
            out.push('…');
        }
        out
    }
}
