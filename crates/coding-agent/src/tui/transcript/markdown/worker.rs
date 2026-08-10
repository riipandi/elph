//! Background parse jobs for assistant markdown (non-blocking UI).

use elph_tui::MarkdownDocument;
use elph_tui::parse_markdown_document;

use super::buffer::AssistantMarkdownBuffer;
use super::buffer::stable_source_hash;
use crate::tui::transcript::retention::MARKDOWN_DOCUMENT_WINDOW;
use crate::tui::transcript::types::{TranscriptMessage, TranscriptStyle};

/// One CPU-bound parse scheduled off the UI thread.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarkdownParseJob {
    pub message_index: usize,
    pub source: String,
    pub source_hash: u64,
}

/// How many trailing assistant messages to re-partition each tick (streaming + recent).
const PARTITION_TAIL_ASSISTANTS: usize = 3;

/// Partition-only refresh for recent assistant messages (full history is stable once complete).
pub fn partition_assistant_markdown(messages: &mut [TranscriptMessage], screen_width: u16) -> bool {
    let mut changed = false;
    let mut seen_assistants = 0usize;
    for message in messages.iter_mut().rev() {
        if message.style != TranscriptStyle::Assistant {
            continue;
        }
        seen_assistants += 1;
        // Always refresh incomplete streams; only the last few completed replies need touch-ups.
        let streaming =
            message.duration_secs.is_none() || message.markdown.as_ref().is_some_and(|buffer| !buffer.stream_complete);
        if !streaming && seen_assistants > PARTITION_TAIL_ASSISTANTS {
            continue;
        }
        let wrap_width = message.content_inner_width(screen_width);
        if message.markdown.is_none() {
            message.markdown = Some(std::sync::Arc::new(AssistantMarkdownBuffer::new()));
        }
        if let Some(buffer) = message.markdown.as_mut()
            && std::sync::Arc::make_mut(buffer).refresh_stable(&message.content, wrap_width)
        {
            changed = true;
        }
        if !streaming && seen_assistants >= PARTITION_TAIL_ASSISTANTS {
            break;
        }
    }
    changed
}

/// Collect parse jobs for stable slices that lack a cached document (newest first).
///
/// Bounded to the same trailing window retention keeps resident
/// ([`MARKDOWN_DOCUMENT_WINDOW`]): parsing older rows here would refill exactly the caches
/// retention releases each turn, so the two would fight every tick. Rows outside the window
/// re-derive their document on paint instead.
pub fn collect_markdown_parse_jobs(messages: &[TranscriptMessage]) -> Vec<MarkdownParseJob> {
    let mut jobs = Vec::new();
    let parse_from = messages.len().saturating_sub(MARKDOWN_DOCUMENT_WINDOW);
    for (index, message) in messages.iter().enumerate().rev() {
        if index < parse_from {
            break;
        }
        if message.style != TranscriptStyle::Assistant {
            continue;
        }
        let Some(buffer) = message.markdown.as_ref() else {
            continue;
        };
        if !buffer.needs_parse() {
            continue;
        }
        let Some(part) = buffer.parts.first() else {
            continue;
        };
        // Char-safe slice: never panic on multi-byte content if source_end drifts.
        let Some(source) = message.content.get(..part.source_end).map(str::to_string) else {
            continue;
        };
        jobs.push(MarkdownParseJob {
            message_index: index,
            source,
            source_hash: part.source_hash,
        });
        if jobs.len() >= PARTITION_TAIL_ASSISTANTS {
            break;
        }
    }
    jobs
}

/// Apply a background parse result if the stable slice is unchanged.
pub fn apply_markdown_parse_result(
    messages: &mut [TranscriptMessage],
    job: &MarkdownParseJob,
    document: MarkdownDocument,
) -> bool {
    let Some(message) = messages.get_mut(job.message_index) else {
        return false;
    };
    if message.style != TranscriptStyle::Assistant {
        return false;
    }
    let stable = message
        .content
        .get(..message.markdown.as_ref().map(|b| b.stable_end).unwrap_or(0));
    let Some(stable) = stable else {
        return false;
    };
    if stable_source_hash(stable) != job.source_hash {
        return false;
    }
    let Some(buffer) = message.markdown.as_mut() else {
        return false;
    };
    std::sync::Arc::make_mut(buffer).apply_document(job.source_hash, document)
}

/// Parse on a worker thread (safe to call inside `spawn_blocking`).
pub fn parse_markdown_on_worker(source: &str) -> MarkdownDocument {
    parse_markdown_document(source)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::transcript::types::TranscriptMessage;

    fn large_tools_table_markdown() -> String {
        let mut lines = vec![
            "## Available tools (build mode, 40 active)".to_string(),
            String::new(),
            "| Tool | Group | Description |".to_string(),
            "| --- | --- | --- |".to_string(),
        ];
        for index in 0..40 {
            lines.push(format!(
                "| `tool_{index:02}` | Group | Description text for tool {index:02} that is long enough to bulk the table past the streaming-tail plain-wrap threshold |"
            ));
        }
        lines.join("\n")
    }

    #[test]
    fn slash_tools_table_freezes_full_document_for_gfm_parse() {
        let content = large_tools_table_markdown();
        assert!(
            content.len() > 800,
            "fixture must exceed streaming-tail markdown max so plain wrap would skip tables"
        );

        let mut messages = vec![TranscriptMessage::assistant_slash_markdown(content.clone())];
        assert!(partition_assistant_markdown(&mut messages, 100));

        let buffer = messages[0].markdown.as_ref().expect("markdown buffer");
        assert_eq!(
            buffer.stable_end,
            content.len(),
            "completed slash markdown must freeze the whole table, not leave it in the tail"
        );
        assert!(buffer.tail(&content).is_empty());

        let jobs = collect_markdown_parse_jobs(&messages);
        assert_eq!(jobs.len(), 1);
        let document = parse_markdown_on_worker(&jobs[0].source);
        assert!(
            document.lines.iter().any(|line| line.table.is_some()),
            "expected a parsed GFM table in the stable document"
        );
        assert!(apply_markdown_parse_result(&mut messages, &jobs[0], document));
        let buffer = messages[0].markdown.as_ref().expect("markdown buffer");
        assert!(
            buffer.parts[0]
                .document
                .as_ref()
                .is_some_and(|doc| { doc.lines.iter().any(|line| line.table.is_some()) })
        );
    }

    #[test]
    fn incomplete_assistant_stream_keeps_table_in_tail() {
        // Table with a header separator is syntactically complete — it freezes into the stable
        // prefix even without a trailing blank line, so it can't be truncated by the tail cap.
        let content = "## Tools\n\n| Tool | Group |\n| --- | --- |\n| `a` | G |\n| `b` | G |";
        let mut messages = vec![TranscriptMessage::assistant_markdown(content.to_string())];
        let _ = partition_assistant_markdown(&mut messages, 100);
        let buffer = messages[0].markdown.as_ref().expect("markdown buffer");
        assert_eq!(
            buffer.stable_end,
            content.len(),
            "complete table must freeze into stable prefix, not linger in tail"
        );
        assert!(buffer.tail(content).is_empty());
    }
}
