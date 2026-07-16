//! Background parse jobs for assistant markdown (non-blocking UI).

use elph_tui::{MarkdownDocument, parse_markdown_document};

use super::buffer::{AssistantMarkdownBuffer, stable_source_hash};
use crate::tui::transcript::types::{TranscriptMessage, TranscriptStyle};

/// One CPU-bound parse scheduled off the UI thread.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarkdownParseJob {
    pub message_index: usize,
    pub source: String,
    pub source_hash: u64,
}

/// Partition-only refresh for all assistant messages.
pub fn partition_assistant_markdown(messages: &mut [TranscriptMessage], screen_width: u16) -> bool {
    let mut changed = false;
    for message in messages.iter_mut() {
        if message.style != TranscriptStyle::Assistant {
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
    }
    changed
}

/// Collect parse jobs for stable slices that lack a cached document.
pub fn collect_markdown_parse_jobs(messages: &[TranscriptMessage]) -> Vec<MarkdownParseJob> {
    let mut jobs = Vec::new();
    for (index, message) in messages.iter().enumerate() {
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
        let source = message.content[..part.source_end].to_string();
        jobs.push(MarkdownParseJob {
            message_index: index,
            source,
            source_hash: part.source_hash,
        });
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
