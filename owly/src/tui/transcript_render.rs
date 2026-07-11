//! Formats [`OwlyEntry`] rows for the tuie transcript pane.

use crate::tui::entries::{OwlyEntry, OwlyEntryKind};
use crate::tui::tool_display::{tool_transcript_body, tool_transcript_compact};
use elph_tui::{ToolExecutionState, ToolExecutionStatus};

const TOOL_ARGS_MAX: usize = 48;
const TOOL_PREVIEW_MAX: usize = 56;

/// Flatten typed entries into scrollable transcript lines.
pub fn entries_to_lines(
    entries: &[OwlyEntry],
    show_thinking: bool,
    agent_running: bool,
    live_tools: &[ToolExecutionState],
) -> Vec<String> {
    let mut lines = Vec::new();
    let mut prev_kind: Option<OwlyEntryKind> = None;

    for (index, entry) in entries.iter().enumerate() {
        if should_skip_while_running(entries, index, agent_running) {
            continue;
        }
        let gap = section_gap(prev_kind, entry.kind);
        for _ in 0..gap {
            lines.push(String::new());
        }
        prev_kind = Some(entry.kind);
        lines.extend(entry_lines(entry, show_thinking));
    }

    if agent_running && !live_tools.is_empty() {
        if !lines.is_empty() {
            lines.push(String::new());
        }
        for tool in live_tools {
            lines.push(format_live_tool(tool));
        }
    }

    lines
}

fn should_skip_while_running(entries: &[OwlyEntry], index: usize, agent_running: bool) -> bool {
    if !agent_running {
        return false;
    }
    let entry = &entries[index];
    if entry.kind != OwlyEntryKind::Assistant || entry.inner.is_streaming {
        return false;
    }
    entries[..index]
        .iter()
        .any(|e| e.kind == OwlyEntryKind::Assistant && e.inner.is_streaming)
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

fn format_live_tool(tool: &ToolExecutionState) -> String {
    let mut line = tool_transcript_compact(tool, TOOL_ARGS_MAX, TOOL_PREVIEW_MAX);
    if tool.status == ToolExecutionStatus::Running {
        line.push_str("  …");
    }
    line
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
    use crate::tui::entries::{OwlyEntry, command_result_entry};
    use elph_tui::ToolExecutionState;

    #[test]
    fn user_lines_use_prompt_prefix() {
        let lines = entries_to_lines(&[OwlyEntry::user("hello")], false, false, &[]);
        assert_eq!(lines, vec!["❯ hello".to_string()]);
    }

    #[test]
    fn multiline_user_indents_continuation() {
        let lines = entries_to_lines(&[OwlyEntry::user("line one\nline two")], false, false, &[]);
        assert_eq!(lines, vec!["❯ line one".to_string(), "  line two".to_string()]);
    }

    #[test]
    fn streaming_assistant_renders_partial_while_running() {
        let entries = vec![
            OwlyEntry::user("go"),
            OwlyEntry::assistant_streaming("partial"),
            OwlyEntry::assistant("done"),
        ];
        let lines = entries_to_lines(&entries, false, true, &[]);
        assert!(lines.iter().any(|l| l.contains("❯ go")));
        assert!(lines.iter().any(|l| l.contains("partial")));
        assert!(!lines.iter().any(|l| l == "done"));
    }

    #[test]
    fn live_tools_render_while_running() {
        let tool = ToolExecutionState::new("1", "bash")
            .with_args("ls")
            .with_status(ToolExecutionStatus::Running);
        let lines = entries_to_lines(&[OwlyEntry::user("go")], false, true, std::slice::from_ref(&tool));
        assert!(lines.iter().any(|l| l.contains("bash") && l.ends_with('…')));
    }

    #[test]
    fn thinking_hidden_when_disabled() {
        let lines = entries_to_lines(&[OwlyEntry::thinking("plan")], false, false, &[]);
        assert!(lines.is_empty());
    }

    #[test]
    fn thinking_shown_when_enabled() {
        let lines = entries_to_lines(&[OwlyEntry::thinking("plan")], true, false, &[]);
        assert_eq!(lines, vec!["Thinking…".to_string()]);
    }

    #[test]
    fn hint_renders_without_prefix() {
        let lines = entries_to_lines(&[OwlyEntry::hint("Owly v0.0.6")], true, false, &[]);
        assert_eq!(lines, vec!["Owly v0.0.6".to_string()]);
    }

    #[test]
    fn command_result_preserves_checkmark() {
        let lines = entries_to_lines(&[command_result_entry("done", true)], true, false, &[]);
        assert_eq!(lines.len(), 1);
        assert!(lines[0].starts_with('✓'));
    }

    #[test]
    fn tool_summary_renders_compact_line() {
        let tool = ToolExecutionState::new("1", "bash").with_args("ls");
        let lines = entries_to_lines(&[OwlyEntry::tool_summary(tool)], false, false, &[]);
        assert_eq!(lines.len(), 1);
        assert!(lines[0].contains("bash"));
    }

    #[test]
    fn tool_summary_includes_body_when_thinking_enabled() {
        let tool = ToolExecutionState::new("1", "bash").with_args("ls -la");
        let lines = entries_to_lines(&[OwlyEntry::tool_summary(tool)], true, false, &[]);
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("bash"));
        assert!(lines[1].contains("ls -la"));
    }

    #[test]
    fn section_gap_inserts_blank_between_turns() {
        let entries = vec![
            OwlyEntry::user("first"),
            OwlyEntry::assistant("reply"),
            OwlyEntry::user("second"),
        ];
        let lines = entries_to_lines(&entries, false, false, &[]);
        assert_eq!(
            lines,
            vec![
                "❯ first".to_string(),
                String::new(),
                "reply".to_string(),
                String::new(),
                "❯ second".to_string(),
            ]
        );
    }
}
