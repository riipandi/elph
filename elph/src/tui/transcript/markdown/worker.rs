//! Background parse jobs for assistant markdown (non-blocking UI).

use elph_tui::MarkdownDocument;
use elph_tui::parse_markdown_document;

use super::buffer::AssistantMarkdownBuffer;
use super::buffer::stable_source_hash;
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
        let wrap_width = elph_tui::transcript_bubble_inner_width(screen_width, message.style.horizontal_padding());
        if message.markdown.is_none() {
            message.markdown = Some(AssistantMarkdownBuffer::new());
        }
        if let Some(buffer) = message.markdown.as_mut()
            && buffer.refresh_stable(&message.content, wrap_width)
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
pub fn collect_markdown_parse_jobs(messages: &[TranscriptMessage]) -> Vec<MarkdownParseJob> {
    let mut jobs = Vec::new();
    for (index, message) in messages.iter().enumerate().rev() {
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
    buffer.apply_document(job.source_hash, document)
}

/// Parse on a worker thread (safe to call inside `spawn_blocking`).
pub fn parse_markdown_on_worker(source: &str) -> MarkdownDocument {
    parse_markdown_document(source)
}
