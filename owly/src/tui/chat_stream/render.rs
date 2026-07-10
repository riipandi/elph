use elph_tui::{
    ScrollSnapshot, Theme, ToolExecutionState, ToolExecutionStatus, apply_transcript_auto_scroll,
    handle_transcript_scroll_keys, prepare_transcript_follow, render_assistant_message,
    unpin_auto_scroll_if_scrolled_up,
};
use slt::{Color, Context};

use super::OwlyChatState;
use crate::tui::entries::{OwlyEntry, OwlyEntryKind};
use crate::tui::tool_display::{tool_transcript_body, tool_transcript_compact};

const LINE_SCROLL_STEP: usize = 3;
const PAGE_SCROLL_STEP: usize = 3;
const TOOL_ARGS_MAX: usize = 48;
const TOOL_PREVIEW_MAX: usize = 56;

fn entries_follow_tail(entries: &[OwlyEntry], agent_running: bool) -> bool {
    agent_running
        || entries
            .iter()
            .any(|entry| entry.kind == OwlyEntryKind::Assistant && entry.inner.is_streaming)
}

pub fn render_owly_chat_stream(
    ui: &mut Context,
    state: &mut OwlyChatState,
    entries: &[OwlyEntry],
    live_tools: &[ToolExecutionState],
    theme: Theme,
    show_thinking: bool,
    agent_running: bool,
) {
    let snapshot = ScrollSnapshot::capture(&state.scroll);

    if state.scroll_enabled {
        handle_transcript_scroll_keys(
            ui,
            &mut state.scroll,
            &mut state.auto_scroll,
            LINE_SCROLL_STEP,
            PAGE_SCROLL_STEP,
        );
    }

    let follow_tail = entries_follow_tail(entries, agent_running);

    if state.scroll_enabled {
        prepare_transcript_follow(&mut state.scroll, state.auto_scroll, follow_tail, snapshot);
    }

    let _ = ui.scroll_col(&mut state.scroll, |ui| {
        let mut prev_kind = None;
        for entry in entries {
            render_entry(ui, entry, theme, show_thinking, agent_running, &mut prev_kind);
        }
        if agent_running {
            render_live_tools(ui, live_tools, theme);
        }
    });

    if state.scroll_enabled {
        unpin_auto_scroll_if_scrolled_up(&state.scroll, &mut state.auto_scroll, snapshot);
        apply_transcript_auto_scroll(&mut state.scroll, &mut state.auto_scroll, snapshot, follow_tail);
    }
}

fn render_live_tools(ui: &mut Context, live_tools: &[ToolExecutionState], theme: Theme) {
    for tool in live_tools {
        let line = tool_transcript_compact(tool, TOOL_ARGS_MAX, TOOL_PREVIEW_MAX);
        let color = tool_summary_color(tool.status).unwrap_or(theme.muted);
        let _ = ui.text(line).fg(color);
    }
}

fn render_entry(
    ui: &mut Context,
    entry: &OwlyEntry,
    theme: Theme,
    show_thinking: bool,
    agent_running: bool,
    prev_kind: &mut Option<OwlyEntryKind>,
) {
    if should_skip_entry(entry.kind) {
        return;
    }

    let gap = section_gap(*prev_kind, entry.kind);
    if gap > 0 {
        for _ in 0..gap {
            let _ = ui.text("");
        }
    }
    *prev_kind = Some(entry.kind);

    match entry.kind {
        OwlyEntryKind::Hint => {
            let content = entry.inner.content.trim();
            if !content.is_empty() {
                let _ = ui.text(content).fg(theme.muted);
            }
        }
        OwlyEntryKind::User => {
            let text = format_user(&entry.inner.content);
            if let Some(c) = theme.text_color() {
                let _ = ui.text(text).bold().fg(c);
            } else {
                let _ = ui.text(text).bold();
            }
        }
        OwlyEntryKind::Assistant => {
            render_assistant_message(
                ui,
                &entry.inner.content,
                entry.inner.is_streaming && agent_running,
                theme,
            );
        }
        OwlyEntryKind::Thinking if show_thinking => {
            let label = if entry.inner.thinking_expanded {
                format!("Thinking:\n{}", entry.inner.content)
            } else {
                "Thinking…".to_string()
            };
            let _ = ui.text(label).fg(theme.muted);
        }
        OwlyEntryKind::Thinking => {}
        OwlyEntryKind::Status => {}
        OwlyEntryKind::CommandResult => {
            let content = &entry.inner.content;
            if let Some(c) = command_result_color(content) {
                let _ = ui.text(content.clone()).fg(c);
            } else {
                let _ = ui.text(content.clone());
            }
        }
        OwlyEntryKind::ToolSummary => {
            if let Some(tool) = &entry.inner.tool {
                let line = tool_transcript_compact(tool, TOOL_ARGS_MAX, TOOL_PREVIEW_MAX);
                if let Some(c) = tool_summary_color(tool.status) {
                    let _ = ui.text(line).fg(c);
                } else {
                    let _ = ui.text(line);
                }
                if show_thinking && let Some(body) = tool_transcript_body(tool) {
                    let _ = ui.text(body).fg(theme.muted);
                }
            }
        }
    }
}

fn should_skip_entry(kind: OwlyEntryKind) -> bool {
    matches!(kind, OwlyEntryKind::Status)
}

fn section_gap(prev: Option<OwlyEntryKind>, current: OwlyEntryKind) -> u32 {
    let Some(prev) = prev else {
        return 0;
    };
    if should_skip_entry(current) || should_skip_entry(prev) {
        return 0;
    }
    if prev == current {
        return match current {
            OwlyEntryKind::ToolSummary => 0,
            _ => 0,
        };
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

fn command_result_color(content: &str) -> Option<Color> {
    if content.starts_with('✓') {
        Some(Color::Green)
    } else if content.starts_with('✗') {
        Some(Color::Red)
    } else {
        None
    }
}

fn tool_summary_color(status: ToolExecutionStatus) -> Option<Color> {
    match status {
        ToolExecutionStatus::Success => Some(Color::Green),
        ToolExecutionStatus::Error => Some(Color::Red),
        ToolExecutionStatus::Running | ToolExecutionStatus::Pending => Some(Color::Cyan),
        ToolExecutionStatus::Cancelled => Some(Color::Yellow),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use elph_tui::{ToolExecutionState, ToolExecutionStatus};
    use slt::TestBackend;

    #[test]
    fn tool_transcript_renders_full_args_in_body() {
        let tool = ToolExecutionState::new("1", "bash")
            .with_args("ls -la /very/long/path/that/would/previously/be/truncated")
            .with_status(ToolExecutionStatus::Success);
        let body = crate::tui::tool_display::tool_transcript_body(&tool).expect("body");
        assert!(body.contains("ls -la /very/long/path/that/would/previously/be/truncated"));
    }

    #[test]
    fn section_gap_adds_space_before_user_turn() {
        assert_eq!(section_gap(Some(OwlyEntryKind::Assistant), OwlyEntryKind::User), 1);
        assert_eq!(section_gap(Some(OwlyEntryKind::Status), OwlyEntryKind::Status), 0);
    }

    #[test]
    fn follow_tail_when_streaming() {
        let entries = vec![OwlyEntry::assistant_streaming("typing")];
        assert!(entries_follow_tail(&entries, false));
        assert!(entries_follow_tail(&[], true));
        assert!(!entries_follow_tail(&[OwlyEntry::assistant("done")], false));
    }

    fn render_like_app(ui: &mut slt::Context, state: &mut OwlyChatState, entries: &[OwlyEntry], theme: Theme) {
        let _ = ui.container().grow(1).col(|ui| {
            let _ = ui.container().grow(1).col(|ui| {
                render_owly_chat_stream(ui, state, entries, &[], theme, false, false);
            });
            for _ in 0..4 {
                let _ = ui.text("prompt");
            }
        });
    }

    #[test]
    fn transcript_scrolls_inside_bounded_viewport() {
        let mut backend = TestBackend::new(60, 14);
        let mut state = OwlyChatState {
            auto_scroll: false,
            ..Default::default()
        };
        let entries: Vec<OwlyEntry> = (0..40).map(|i| OwlyEntry::user(format!("chat line {i:02}"))).collect();
        let theme = Theme::dark();

        for _ in 0..2 {
            backend.render(|ui| render_like_app(ui, &mut state, &entries, theme));
        }

        let viewport = state.scroll.viewport_height();
        let content = state.scroll.content_height();
        assert!(
            viewport > 0 && content > viewport,
            "expected scrollable overflow (viewport={viewport}, content={content})"
        );

        state.scroll.set_offset(content.saturating_sub(viewport) as usize);
        backend.render(|ui| render_like_app(ui, &mut state, &entries, theme));
        backend.assert_contains("chat line 39");
    }

    #[test]
    fn shell_grow_bounds_chat_viewport() {
        let mut backend = TestBackend::new(60, 20);
        let mut state = OwlyChatState::default();
        let entries: Vec<OwlyEntry> = (0..30).map(|i| OwlyEntry::user(format!("line {i}"))).collect();
        let theme = Theme::dark();

        for _ in 0..2 {
            backend.render(|ui| render_like_app(ui, &mut state, &entries, theme));
        }

        assert!(
            state.scroll.viewport_height() > 0 && state.scroll.content_height() > state.scroll.viewport_height(),
            "chat stream should scroll inside a bounded viewport"
        );
    }
}
