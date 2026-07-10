//! Flush Owly transcript rows into terminal scrollback via [`Context::static_log`].

use slt::Context;

use crate::tui::entries::{OwlyEntry, OwlyEntryKind};
use crate::tui::tool_display::{tool_transcript_body, tool_transcript_compact};

const TOOL_ARGS_MAX: usize = 48;
const TOOL_PREVIEW_MAX: usize = 56;

/// Tracks how much of the transcript has been written to scrollback.
#[derive(Debug, Default)]
pub struct TranscriptFlushState {
    pub flushed_entries: usize,
}

pub fn emit_banner(ui: &mut Context, lines: &[String]) {
    for line in lines {
        ui.static_log(line.as_str());
    }
}

pub fn sync_transcript(
    ui: &mut Context,
    state: &mut TranscriptFlushState,
    entries: &[OwlyEntry],
    show_thinking: bool,
    agent_running: bool,
) {
    let mut prev_kind = if state.flushed_entries == 0 {
        None
    } else {
        entries.get(state.flushed_entries.saturating_sub(1)).map(|e| e.kind)
    };

    for entry in entries.iter().skip(state.flushed_entries) {
        if should_hold_for_streaming(entry, agent_running) {
            break;
        }
        flush_entry(ui, entry, show_thinking, &mut prev_kind);
        state.flushed_entries += 1;
    }
}

fn should_hold_for_streaming(entry: &OwlyEntry, agent_running: bool) -> bool {
    agent_running && entry.kind == OwlyEntryKind::Assistant && entry.inner.is_streaming
}

fn flush_entry(ui: &mut Context, entry: &OwlyEntry, show_thinking: bool, prev_kind: &mut Option<OwlyEntryKind>) {
    if matches!(entry.kind, OwlyEntryKind::Status) {
        return;
    }

    let gap = section_gap(*prev_kind, entry.kind);
    for _ in 0..gap {
        ui.static_log("");
    }
    *prev_kind = Some(entry.kind);

    for line in entry_lines(entry, show_thinking) {
        ui.static_log(line);
    }
}

fn entry_lines(entry: &OwlyEntry, show_thinking: bool) -> Vec<String> {
    match entry.kind {
        OwlyEntryKind::Hint => {
            let content = entry.inner.content.trim();
            if content.is_empty() {
                Vec::new()
            } else {
                vec![content.to_string()]
            }
        }
        OwlyEntryKind::User => format_user(&entry.inner.content).lines().map(str::to_string).collect(),
        OwlyEntryKind::Assistant => entry.inner.content.lines().map(str::to_string).collect(),
        OwlyEntryKind::Thinking if show_thinking => {
            if entry.inner.thinking_expanded {
                std::iter::once("Thinking:".to_string())
                    .chain(entry.inner.content.lines().map(str::to_string))
                    .collect()
            } else {
                vec!["Thinking…".to_string()]
            }
        }
        OwlyEntryKind::Thinking => Vec::new(),
        OwlyEntryKind::Status => Vec::new(),
        OwlyEntryKind::CommandResult => vec![entry.inner.content.clone()],
        OwlyEntryKind::ToolSummary => {
            let mut lines = Vec::new();
            if let Some(tool) = &entry.inner.tool {
                lines.push(tool_transcript_compact(tool, TOOL_ARGS_MAX, TOOL_PREVIEW_MAX));
                if show_thinking && let Some(body) = tool_transcript_body(tool) {
                    lines.extend(body.lines().map(str::to_string));
                }
            }
            lines
        }
    }
}

fn section_gap(prev: Option<OwlyEntryKind>, current: OwlyEntryKind) -> u32 {
    let Some(prev) = prev else {
        return 0;
    };
    if matches!(current, OwlyEntryKind::Status) || matches!(prev, OwlyEntryKind::Status) {
        return 0;
    }
    match (prev, current) {
        (OwlyEntryKind::User, _) | (_, OwlyEntryKind::User) => 1,
        (OwlyEntryKind::Assistant, OwlyEntryKind::Assistant) => 0,
        _ => 1,
    }
}

fn format_user(message: &str) -> String {
    let trimmed = message.trim_end();
    let mut lines = trimmed.lines();
    let Some(first) = lines.next() else {
        return String::new();
    };
    let mut out = format!("❯ {first}");
    for line in lines {
        out.push('\n');
        out.push_str("  ");
        out.push_str(line);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use elph_tui::ToolExecutionState;
    use slt::TestBackend;

    fn with_ui<F: FnOnce(&mut slt::Context)>(f: F) {
        let mut backend = TestBackend::new(80, 24);
        backend.render(|ui| f(ui));
    }

    #[test]
    fn flushes_user_and_assistant_entries() {
        let mut state = TranscriptFlushState::default();
        let entries = vec![OwlyEntry::user("hello"), OwlyEntry::assistant("world")];

        with_ui(|ui| sync_transcript(ui, &mut state, &entries, false, false));

        assert_eq!(state.flushed_entries, 2);
    }

    #[test]
    fn holds_streaming_assistant_until_complete() {
        let mut state = TranscriptFlushState::default();
        let streaming = vec![OwlyEntry::user("go"), OwlyEntry::assistant_streaming("partial")];

        with_ui(|ui| sync_transcript(ui, &mut state, &streaming, false, true));
        assert_eq!(state.flushed_entries, 1);

        let done = vec![OwlyEntry::user("go"), OwlyEntry::assistant("done")];
        with_ui(|ui| sync_transcript(ui, &mut state, &done, false, false));
        assert_eq!(state.flushed_entries, 2);
    }

    #[test]
    fn tool_summary_advances_flush_cursor() {
        let mut state = TranscriptFlushState::default();
        let tool = ToolExecutionState::new("1", "bash").with_args("ls");
        let entries = vec![OwlyEntry::tool_summary(tool)];

        with_ui(|ui| sync_transcript(ui, &mut state, &entries, false, false));

        assert_eq!(state.flushed_entries, 1);
    }
}
