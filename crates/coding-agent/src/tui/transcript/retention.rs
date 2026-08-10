//! Memory retention for the live transcript.
//!
//! The transcript is the user's scrollback: every row stays mounted for the whole session.
//! Retention therefore never drops rows — it only sheds *derived* or *bounded* payloads that
//! can be rebuilt or that the resumed session would not show anyway:
//!
//! - Parsed `MarkdownDocument` caches outside the recent window. The source text stays in
//!   `TranscriptMessage::content`, and the paint path re-derives (and globally caches) the
//!   document when the row scrolls back into view, so nothing changes on screen.
//! - Tool diff text (`old_text` / `new_text`) past a byte budget, newest-first. Diffs are the
//!   single largest per-message payload (a full before/after file copy each).
//!
//! Dropping whole messages here is what previously made older transcript rows vanish mid-session
//! and reappear only after a restart (resume rebuilds them from the session tree).

use std::sync::Arc;

use super::types::TranscriptMessage;

/// Trailing messages that keep their parsed markdown documents resident.
///
/// Covers the viewport plus scroll overscan, so scrolling a screen or two back never waits
/// on a re-parse. Older rows re-derive their document on paint (see `build_cached_document`).
///
/// The background parse worker uses the same window (`collect_markdown_parse_jobs`); if it
/// re-parsed rows that retention then released, the two would ping-pong every turn.
pub(crate) const MARKDOWN_DOCUMENT_WINDOW: usize = 40;

/// Byte budget for retained tool diff text across the whole transcript.
///
/// Walked newest-first: recent edits keep their inline diff, older ones fall back to the
/// collapsed card (the same thing a resumed session shows before its details reload).
const DIFF_TEXT_BUDGET_BYTES: usize = 8 * 1024 * 1024;

/// Shed re-derivable memory from the live transcript. Returns `true` when anything changed.
///
/// Never removes, reorders, or rewrites message content — only caches and diff payloads.
pub fn apply_transcript_retention(messages: &mut [TranscriptMessage]) -> bool {
    apply_transcript_retention_with(messages, MARKDOWN_DOCUMENT_WINDOW, DIFF_TEXT_BUDGET_BYTES)
}

/// [`apply_transcript_retention`] with explicit limits (tests pin small windows).
fn apply_transcript_retention_with(
    messages: &mut [TranscriptMessage],
    markdown_window: usize,
    diff_budget_bytes: usize,
) -> bool {
    let markdown_keep_from = messages.len().saturating_sub(markdown_window);
    let mut diff_budget = diff_budget_bytes;
    let mut changed = false;

    // Newest-first so the budget is spent on the rows the user is most likely to revisit.
    for (index, message) in messages.iter_mut().enumerate().rev() {
        if let Some(tool) = message.tool.as_mut() {
            let diff_size = tool.diff_text_size();
            if diff_size > 0 {
                if diff_size <= diff_budget {
                    diff_budget -= diff_size;
                } else {
                    diff_budget = 0;
                    tool.strip_diff_text();
                    changed = true;
                }
            }
        }

        if index >= markdown_keep_from {
            continue;
        }
        if let Some(markdown) = message.markdown.as_mut()
            && markdown.has_cached_documents()
        {
            // Rebuild without the documents instead of `Arc::make_mut`: the buffer is shared
            // with the render snapshot, so make_mut would deep-clone every document we are
            // about to free — a memory spike exactly when reclaiming memory.
            *markdown = Arc::new(markdown.without_documents());
            changed = true;
        }
    }

    changed
}

#[cfg(test)]
mod tests {
    use super::super::markdown::AssistantMarkdownBuffer;
    use super::super::types::{TranscriptMessage, TranscriptStyle};
    use super::*;

    fn assistant_with_document(content: &str) -> TranscriptMessage {
        let mut message = TranscriptMessage::text(content, TranscriptStyle::Assistant);
        let mut buffer = AssistantMarkdownBuffer::new();
        buffer.mark_stream_complete();
        buffer.refresh_stable(content, 80);
        if let Some(part) = buffer.parts.first() {
            let hash = part.source_hash;
            let document = elph_tui::parse_markdown_document(content);
            buffer.apply_document(hash, document);
        }
        message.markdown = Some(Arc::new(buffer));
        message
    }

    fn edit_tool(bytes: usize) -> TranscriptMessage {
        let mut message = TranscriptMessage::tool_call("edit_file", r#"{"path":"a.rs"}"#, TranscriptStyle::ToolSuccess);
        let tool = message.tool.as_mut().expect("tool");
        tool.old_text = Some("o".repeat(bytes));
        tool.new_text = Some("n".repeat(bytes));
        tool.file_path = Some("/tmp/a.rs".into());
        message
    }

    #[test]
    fn retention_keeps_every_message() {
        let mut messages: Vec<_> = (0..200)
            .map(|i| TranscriptMessage::text(format!("row {i}"), TranscriptStyle::User))
            .collect();
        apply_transcript_retention(&mut messages);
        assert_eq!(messages.len(), 200, "retention must never drop transcript rows");
        assert_eq!(messages[0].content, "row 0", "oldest row must stay in place");
        assert_eq!(messages[199].content, "row 199");
    }

    #[test]
    fn retention_drops_old_documents_but_keeps_source_and_metrics() {
        let mut messages: Vec<_> = (0..6)
            .map(|i| assistant_with_document(&format!("## Head {i}\n\nBody paragraph {i}.\n")))
            .collect();
        let stable_end_before = messages[0].markdown.as_ref().expect("md").stable_end;
        let rows_before = messages[0].markdown.as_ref().expect("md").parts[0].row_count;

        assert!(apply_transcript_retention_with(&mut messages, 2, usize::MAX));

        let old = messages[0].markdown.as_ref().expect("md");
        assert!(!old.has_cached_documents(), "old document must be released");
        assert_eq!(old.stable_end, stable_end_before, "layout metadata must survive");
        assert_eq!(old.parts[0].row_count, rows_before, "row metrics must survive");
        assert!(messages[0].content.contains("Body paragraph 0."), "source text stays");

        let recent = messages[5].markdown.as_ref().expect("md");
        assert!(recent.has_cached_documents(), "recent document stays resident");
    }

    #[test]
    fn retention_keeps_recent_diffs_and_strips_beyond_budget() {
        let mut messages = vec![edit_tool(1_000), edit_tool(1_000), edit_tool(1_000)];
        // Budget fits only the newest message (each holds old+new = 2_000 bytes).
        assert!(apply_transcript_retention_with(&mut messages, usize::MAX, 2_000));

        assert!(!messages[0].tool.as_ref().expect("tool").has_inline_diff());
        assert!(!messages[1].tool.as_ref().expect("tool").has_inline_diff());
        assert!(
            messages[2].tool.as_ref().expect("tool").has_inline_diff(),
            "newest diff stays within budget"
        );
        // Card identity is untouched even when the diff text is gone.
        assert_eq!(messages[0].tool.as_ref().expect("tool").name, "edit_file");
        assert_eq!(messages[0].tool.as_ref().expect("tool").file_path.as_deref(), Some("/tmp/a.rs"));
    }

    #[test]
    fn retention_is_idempotent() {
        let mut messages = vec![
            assistant_with_document("## A\n\nText.\n"),
            assistant_with_document("## B\n\nText.\n"),
            edit_tool(4_000),
        ];
        assert!(apply_transcript_retention_with(&mut messages, 1, 1_000));
        assert!(
            !apply_transcript_retention_with(&mut messages, 1, 1_000),
            "second pass has nothing left to shed"
        );
        assert_eq!(messages.len(), 3);
    }
}
