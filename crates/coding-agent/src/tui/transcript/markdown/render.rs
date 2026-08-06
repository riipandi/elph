//! Paint cached markdown documents into transcript cards.

use std::collections::HashMap;
use std::sync::Mutex;

use elph_tui::MarkdownDocument;
use elph_tui::{render_linkified_plain_text, render_markdown_block, streaming_tail_document};
use iocraft::prelude::*;

use super::buffer::AssistantMarkdownBuffer;

/// Cache key for the built (merged) markdown document per (stable_hash, wrap_width).
///
/// `build_assistant_markdown_document` clones the cached `MarkdownDocument` and merges
/// it with the streaming tail — an O(n) allocation that runs on every paint for every
/// visible assistant message. For completed messages (which don't change), we cache the
/// built document and reuse it across frames. This eliminates the per-frame clone for
/// the vast majority of visible messages in a long session.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct BuiltDocCacheKey {
    stable_hash: u64,
    wrap_width: u16,
    stream_complete: bool,
}

/// Global cache for built documents. Keyed by (stable_hash, wrap_width, stream_complete).
/// Each entry holds a `MarkdownDocument` (a few KB to a few hundred KB). Bounded to 64 entries.
static BUILT_DOC_CACHE: std::sync::OnceLock<Mutex<HashMap<BuiltDocCacheKey, MarkdownDocument>>> =
    std::sync::OnceLock::new();

fn built_doc_cache() -> &'static Mutex<HashMap<BuiltDocCacheKey, MarkdownDocument>> {
    BUILT_DOC_CACHE.get_or_init(|| Mutex::new(HashMap::with_capacity(32)))
}

/// Max cached built documents. When exceeded, half the cache is drained (oldest first).
const BUILT_DOC_CACHE_MAX: usize = 64;

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
/// When no cached document is available, we parse the stable slice as markdown directly.
/// The stable boundary (`find_stable_boundary`) guarantees the slice ends at a safe position,
/// so parsing in isolation is safe — it won't tear a code block, list, or table in half.
///
/// Parsing (rather than plain-text) keeps formatting consistent between the worker-cached
/// document and the fallback: headings, tables, lists, bold/italic, and mermaid diagrams all
/// render identically before and after the worker finishes. (The previous plain-text fallback
/// caused formatting to "jump" when the worker landed — headings lost their styling, tables
/// collapsed, and mermaid lost its diagram.)
fn render_markdown_part(
    document: Option<&MarkdownDocument>,
    fallback_source: &str,
    fallback_foreground: Color,
) -> MarkdownDocument {
    if let Some(doc) = document {
        return doc.clone();
    }
    // Parse the stable slice as markdown so formatting stays consistent. The boundary is
    // guaranteed markdown-safe, so isolated parsing cannot produce structural artifacts.
    let _ = fallback_foreground;
    streaming_tail_document(fallback_source)
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
        // A COMPLETED reply must always render its full content. The tail cap below exists
        // only to keep the live stream cheap; once the stream is done (before the worker
        // re-partitions the buffer, or after a resize/restore), a stale `stable_end` can
        // leave a long tail whose head would be cut by the cap — silently dropping the
        // beginning/middle of an otherwise-finished answer. So cap only while streaming.
        let streaming_tail = !buffer.stream_complete;

        if has_unclosed {
            // In an unclosed codeblock: render the entire tail as markdown
            // to preserve codeblock structure and syntax highlighting.
            let tail_doc = if streaming_tail && tail.len() > 12_000 {
                // Safety cap: only last 12K chars for very long streams
                let start = tail.char_indices().rev().nth(11_999).map(|(i, _)| i).unwrap_or(0);
                streaming_tail_document(&tail[start..])
            } else {
                streaming_tail_document(tail)
            };
            document = merge_documents(document, tail_doc);
        } else {
            // Outside codeblock: use capped tail for performance while streaming.
            // Completed replies render the full tail — no truncation of finished content.
            let capped_tail = if streaming_tail && tail.len() > 4_000 {
                const TAIL_PAINT_MAX: usize = 4_000;
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
    let document = build_cached_document(buffer, raw, tail_foreground, width);
    if document.is_empty() {
        return render_linkified_plain_text(raw, tail_foreground, width);
    }
    render_markdown_block(&document, width)
}

/// Get the built (merged) document, using a cache for completed messages.
///
/// For completed messages (`stream_complete == true` and the stable prefix covers the
/// whole content), the built document is cached per `(stable_hash, wrap_width)`. This
/// avoids cloning the cached `MarkdownDocument` + merging on every paint — an O(n)
/// allocation that was a significant contributor to scroll/resize lag.
///
/// Streaming messages always rebuild (the tail changes every frame).
fn build_cached_document(
    buffer: &AssistantMarkdownBuffer,
    raw: &str,
    tail_foreground: Color,
    wrap_width: u16,
) -> MarkdownDocument {
    // Only cache completed messages whose stable prefix covers the whole content.
    // During streaming, the document changes every frame — caching would waste memory.
    let can_cache = buffer.stream_complete
        && buffer.stable_end >= raw.len()
        && buffer.wrap_width == wrap_width
        && buffer.parts.first().is_some_and(|p| p.document.is_some());

    if can_cache {
        let Some(part) = buffer.parts.first() else {
            return build_assistant_markdown_document(buffer, raw, tail_foreground);
        };
        let stable_hash = part.source_hash;
        let key = BuiltDocCacheKey {
            stable_hash,
            wrap_width,
            stream_complete: true,
        };

        // Cache hit — return cloned cached document.
        if let Ok(cache) = built_doc_cache().lock()
            && let Some(cached) = cache.get(&key)
        {
            return cached.clone();
        }

        // Cache miss — build and cache.
        let doc = build_assistant_markdown_document(buffer, raw, tail_foreground);
        if let Ok(mut cache) = built_doc_cache().lock() {
            if cache.len() >= BUILT_DOC_CACHE_MAX {
                // Drain half (oldest-first via iteration order) while holding the lock,
                // so a concurrent writer can't re-overfill the cache between drain and insert.
                let to_remove = cache.len() / 2;
                let keys: Vec<_> = cache.keys().take(to_remove).copied().collect();
                for k in keys {
                    cache.remove(&k);
                }
            }
            cache.insert(key, doc.clone());
        }
        return doc;
    }

    // Streaming or incomplete — always rebuild.
    build_assistant_markdown_document(buffer, raw, tail_foreground)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::transcript::markdown::buffer::AssistantMarkdownBuffer;

    /// Simulate the streaming→settled transition: a closed mermaid fence moves from the
    /// tail into the stable part, but the worker hasn't parsed it yet (document = None).
    /// The diagram must still render — not collapse into a plain code block.
    #[test]
    fn mermaid_renders_during_worker_parse_window() {
        let raw = "Here is a diagram:\n\n```mermaid\ngraph LR; A[Build] --> B[Deploy]\n```\n";
        let foreground = Color::Reset;

        // Stable boundary has advanced past the closed fence, but worker hasn't parsed yet.
        let mut buffer = AssistantMarkdownBuffer::new();
        buffer.wrap_width = 80;
        buffer.stable_end = raw.len();
        buffer.stream_complete = true;
        // One stable part with NO cached document (worker still parsing).
        buffer.parts = vec![crate::tui::transcript::markdown::buffer::RenderedPart {
            source_end: raw.len(),
            source_hash: 0,
            row_count: 0,
            document: None,
        }];

        let doc = build_assistant_markdown_document(&buffer, raw, foreground);

        // The merged document must contain a deferred mermaid line.
        assert!(
            doc.lines.iter().any(|line| line.mermaid_source.is_some()),
            "mermaid diagram must render even before worker parses, got lines: {:?}",
            doc.lines
                .iter()
                .map(|l| (l.kind, l.mermaid_source.is_some()))
                .collect::<Vec<_>>()
        );
    }

    /// End-to-end: worker finishes parsing and applies the document. The deferred mermaid
    /// line must survive the worker round-trip and render as a diagram.
    #[test]
    fn mermaid_survives_worker_apply_roundtrip() {
        use crate::tui::transcript::markdown::worker::{apply_markdown_parse_result, parse_markdown_on_worker};
        use crate::tui::transcript::types::{TranscriptMessage, TranscriptStyle};

        let raw = "Diagram:\n\n```mermaid\ngraph TD\n    A[Start] --> B[End]\n```\n";
        let mut messages = vec![TranscriptMessage::assistant_markdown(raw.to_string())];

        // Partition to establish stable boundary.
        crate::tui::transcript::markdown::worker::partition_assistant_markdown(&mut messages, 80);

        // Simulate worker: parse the stable source and apply.
        let jobs = crate::tui::transcript::markdown::worker::collect_markdown_parse_jobs(&messages);
        assert!(!jobs.is_empty(), "should have a parse job for stable part");
        let doc = parse_markdown_on_worker(&jobs[0].source);
        assert!(
            doc.lines.iter().any(|l| l.mermaid_source.is_some()),
            "worker-parsed doc must contain deferred mermaid line"
        );
        assert!(apply_markdown_parse_result(&mut messages, &jobs[0], doc));

        // After apply, rendering must still produce a diagram.
        let buffer = messages[0].markdown.as_ref().expect("buffer exists");
        let foreground = TranscriptStyle::Assistant.text_color();
        let merged = build_assistant_markdown_document(buffer, raw, foreground);
        assert!(
            merged.lines.iter().any(|l| l.mermaid_source.is_some()),
            "after worker apply, mermaid must still be present"
        );
    }

    /// Simulate the FULL streaming lifecycle: content grows incrementally, partition runs
    /// after each chunk, worker parses and applies. The diagram must NEVER revert to a
    /// code block at any point in the stream.
    #[test]
    fn mermaid_stays_diagram_across_full_stream() {
        use crate::tui::transcript::markdown::worker::{
            apply_markdown_parse_result, collect_markdown_parse_jobs, parse_markdown_on_worker,
            partition_assistant_markdown,
        };
        use crate::tui::transcript::types::{TranscriptMessage, TranscriptStyle};

        // Simulate chunks arriving like a real LLM stream.
        let chunks = [
            "Here is a diagram:\n\n",
            "```mermaid\n",
            "graph TD\n",
            "    A[Start] --> B{Choice}\n",
            "    B -->|Yes| C[End]\n",
            "    B -->|No| A\n",
            "```\n",
            "\nDone with the diagram.\n",
        ];

        let mut messages = vec![TranscriptMessage::assistant_markdown(String::new())];
        let mut raw = String::new();

        for (i, chunk) in chunks.iter().enumerate() {
            raw.push_str(chunk);
            messages[0].content = raw.clone();

            // Partition may advance the stable boundary.
            let _ = partition_assistant_markdown(&mut messages, 80);

            // Simulate worker: parse any pending jobs and apply.
            loop {
                let jobs = collect_markdown_parse_jobs(&messages);
                if jobs.is_empty() {
                    break;
                }
                let jobs_snapshot = jobs;
                for job in jobs_snapshot {
                    let doc = parse_markdown_on_worker(&job.source);
                    apply_markdown_parse_result(&mut messages, &job, doc);
                }
            }

            // After each chunk, the diagram must be present (if the fence is closed).
            let buffer = messages[0].markdown.as_ref().expect("buffer exists");
            let foreground = TranscriptStyle::Assistant.text_color();
            let merged = build_assistant_markdown_document(buffer, &raw, foreground);

            let has_diagram = merged.lines.iter().any(|l| l.mermaid_source.is_some());
            // Fence is "closed" when we've seen the full ```mermaid ... ``` pair.
            let fence_closed = raw.contains("```mermaid") && count_fences(raw.as_str()).is_multiple_of(2);

            if fence_closed {
                assert!(
                    has_diagram,
                    "chunk {i}: fence closed but diagram lost! raw={raw:?}, lines={:?}",
                    merged
                        .lines
                        .iter()
                        .map(|l| (l.kind, l.mermaid_source.is_some()))
                        .collect::<Vec<_>>()
                );
            }
        }
    }

    /// Count backtick fences (for tests only) — odd = unclosed, even = closed.
    fn count_fences(raw: &str) -> usize {
        let mut count = 0usize;
        let mut pos = 0usize;
        while let Some(rel) = raw[pos..].find("```") {
            count += 1;
            pos = pos + rel + 3;
        }
        count
    }

    /// Simulate a GFM table moving from tail to stable part during streaming. The stable-part
    /// fallback must parse as markdown (not plain text) so the table renders as a grid both
    /// before and after the worker finishes. This guards against "table becomes broken text"
    /// regressions.
    #[test]
    fn table_renders_consistently_during_streaming() {
        let raw = "## Tools\n\n| Tool | Status |\n| --- | --- |\n| grep | ✅ |\n| rg | ✅ |\n";
        let foreground = Color::Reset;

        // Stable boundary advanced past the table, but worker hasn't parsed yet.
        let mut buffer = AssistantMarkdownBuffer::new();
        buffer.wrap_width = 80;
        buffer.stable_end = raw.len();
        buffer.stream_complete = true;
        buffer.parts = vec![crate::tui::transcript::markdown::buffer::RenderedPart {
            source_end: raw.len(),
            source_hash: 0,
            row_count: 0,
            document: None,
        }];

        let doc = build_assistant_markdown_document(&buffer, raw, foreground);

        // The stable part MUST parse as markdown → table lines present with a grid.
        let table_line = doc
            .lines
            .iter()
            .find(|l| matches!(l.kind, elph_tui::MarkdownLineKind::Table))
            .expect("table must render as markdown from stable-part fallback");
        assert!(table_line.table.is_some(), "table line carries its matrix");

        // Heading must also keep its markdown styling.
        assert!(
            doc.lines
                .iter()
                .any(|l| matches!(l.kind, elph_tui::MarkdownLineKind::Heading(2))),
            "heading must render as heading from stable-part fallback"
        );
    }

    /// The uncached stable fallback must produce the SAME document as the worker-parsed cache,
    /// so there's no visible "formatting jump" when the worker finishes.
    #[test]
    fn fallback_matches_worker_document() {
        let raw = "## Tools\n\n| Tool | Status |\n| --- | --- |\n| grep | ✅ |\n| rg | ✅ |\n\n**Done.**\n";
        let foreground = Color::Reset;

        // Fallback (document = None) parses as markdown.
        let mut buffer = AssistantMarkdownBuffer::new();
        buffer.wrap_width = 80;
        buffer.stable_end = raw.len();
        buffer.stream_complete = true;
        buffer.parts = vec![crate::tui::transcript::markdown::buffer::RenderedPart {
            source_end: raw.len(),
            source_hash: 0,
            row_count: 0,
            document: None,
        }];
        let fallback_doc = build_assistant_markdown_document(&buffer, raw, foreground);

        // Worker-parsed (document = Some) path.
        let worker_doc = elph_tui::parse_markdown_document(raw);
        assert_eq!(
            fallback_doc.lines.len(),
            worker_doc.lines.len(),
            "fallback and worker docs must have the same line count (no jump)"
        );
    }

    /// Simulate the ASYNC worker race: the worker collects a job, content grows, and the
    /// worker applies the STALE job afterward. The hash guard must reject the stale apply,
    /// and the diagram must still render.
    #[test]
    fn mermaid_survives_stale_worker_apply() {
        use crate::tui::transcript::markdown::worker::{
            apply_markdown_parse_result, collect_markdown_parse_jobs, parse_markdown_on_worker,
            partition_assistant_markdown,
        };
        use crate::tui::transcript::types::{TranscriptMessage, TranscriptStyle};

        // Phase 1: fence closed, worker collects a job.
        let mut messages = vec![TranscriptMessage::assistant_markdown(
            "Diagram:\n\n```mermaid\nA --> B\n```\n".to_string(),
        )];
        partition_assistant_markdown(&mut messages, 80);
        let jobs = collect_markdown_parse_jobs(&messages);
        assert_eq!(jobs.len(), 1, "one job in phase 1");
        let stale_job = jobs[0].clone();
        let stale_doc = parse_markdown_on_worker(&stale_job.source);
        assert!(
            stale_doc.lines.iter().any(|l| l.mermaid_source.is_some()),
            "stale doc should have mermaid"
        );

        // Phase 2: content grows (new paragraph after the fence) — stable_end advances.
        let grown = "Diagram:\n\n```mermaid\nA --> B\n```\n\nDone with the diagram.\n";
        messages[0].content = grown.to_string();
        partition_assistant_markdown(&mut messages, 80);

        // Worker applies the STALE job now — must be rejected (hash mismatch).
        let applied = apply_markdown_parse_result(&mut messages, &stale_job, stale_doc);
        assert!(!applied, "stale apply must be rejected by hash guard");

        // Diagram must still render.
        let buffer = messages[0].markdown.as_ref().expect("buffer");
        let merged = build_assistant_markdown_document(buffer, grown, TranscriptStyle::Assistant.text_color());
        assert!(
            merged.lines.iter().any(|l| l.mermaid_source.is_some()),
            "diagram must survive stale apply rejection"
        );
    }

    /// A COMPLETED reply must render its full tail — never truncate finished content. The tail
    /// cap exists only to keep live-stream paint cheap; once `stream_complete` is set (but the
    /// worker has not yet re-partitioned, e.g. right after finalize or after a resize/restore),
    /// a stale `stable_end` can leave a long tail. Truncating it would silently drop the middle
    /// of an already-finished answer — the "top content clipped after stream completes" bug.
    #[test]
    fn completed_stream_renders_full_tail_without_cap() {
        let foreground = Color::Reset;

        // Long well-formed reply (> 4K chars) that sits entirely in the tail because the
        // buffer's stable_end is still 0 (worker hasn't re-partitioned after completion).
        let section = "A paragraph of regular text that is long enough to wrap and repeat.\n\n";
        let body = section.repeat(160); // ~ 13K chars — far beyond the 4K tail cap
        let closing = "## Done\n\nFinal paragraph.\n";
        let raw = format!("{body}{closing}");

        let mut buffer = AssistantMarkdownBuffer::new();
        buffer.wrap_width = 80;
        buffer.stable_end = 0; // everything still in tail
        buffer.stream_complete = true; // completed — cap must be bypassed
        buffer.parts = vec![];

        let doc = build_assistant_markdown_document(&buffer, &raw, foreground);
        let text: String = doc
            .lines
            .iter()
            .map(|l| l.spans.iter().map(|s| s.text.as_str()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n");
        // The FILLED body and the closing heading must both be present (no truncation).
        assert!(
            text.contains("## Done") && text.contains("Final paragraph."),
            "completed stream must render the closing content, got tail: {}",
            text.chars().rev().take(80).collect::<String>()
        );
        assert!(text.contains("A paragraph of regular text"), "head content must not be clipped");

        // Same buffer while STILL streaming: cap applies, tail is truncated to ~last 4K chars.
        let mut streaming = buffer.clone();
        streaming.stream_complete = false;
        let streaming_doc = build_assistant_markdown_document(&streaming, &raw, foreground);
        let streaming_text: String = streaming_doc
            .lines
            .iter()
            .map(|l| l.spans.iter().map(|s| s.text.as_str()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            streaming_text.contains("## Done"),
            "streaming tail must still contain the recent tail (closing section)"
        );
        // The head BODY may be dropped while streaming (cap) — but only then.
        assert!(
            streaming_text.matches("A paragraph of regular text").count()
                < text.matches("A paragraph of regular text").count(),
            "while streaming, the cap should drop some repeated body repeats"
        );
    }

    #[test]
    fn built_document_cached_for_completed_messages() {
        use crate::tui::transcript::markdown::buffer::RenderedPart;

        let raw = "## Plan\n\nA paragraph that wraps across the width nicely.\n\n| Col | Val |\n| --- | --- |\n| a | 1 |\n| b | 2 |\n\nLast paragraph.\n";
        let doc = elph_tui::parse_markdown_document(raw);
        let hash = crate::tui::transcript::markdown::buffer::stable_source_hash(raw);

        let buffer = AssistantMarkdownBuffer {
            stable_end: raw.len(),
            parts: vec![RenderedPart {
                source_end: raw.len(),
                source_hash: hash,
                row_count: 1,
                document: Some(doc.clone()),
            }],
            wrap_width: 80,
            stream_complete: true,
        };

        // First call — builds and caches.
        let first = build_cached_document(&buffer, raw, Color::Reset, 80);
        assert!(!first.is_empty());

        // Second call — cache hit, returns cloned cached document.
        let second = build_cached_document(&buffer, raw, Color::Reset, 80);
        assert_eq!(first.lines.len(), second.lines.len());

        // Verify cache has an entry.
        let cache = built_doc_cache().lock().expect("lock");
        assert!(
            !cache.is_empty(),
            "cache should have at least one entry after building, got {}",
            cache.len()
        );
    }
}
