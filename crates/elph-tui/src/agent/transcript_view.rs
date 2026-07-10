use super::assistant_message::render_assistant_message;
use super::detail_block::{CollapseState, render_detail_block, render_pipe_message};
use crate::components::inline_line;
use crate::shell::shell_chat_block_gap;
use crate::theme::Theme;
use crate::transcript::{TranscriptEntry, TranscriptRole};
use slt::Context;

/// Renders a transcript column from the given entries.
pub fn render_transcript_view(
    ui: &mut Context,
    entries: &[TranscriptEntry],
    show_thinking: bool,
    theme: Theme,
    collapse: &CollapseState,
    agent_running: bool,
) {
    let gap = shell_chat_block_gap(ui);
    let _ = ui.container().gap(gap).col(|ui| {
        for (index, entry) in entries.iter().enumerate() {
            render_entry(ui, entry, index, theme, show_thinking, collapse, agent_running);
        }
    });
}

fn render_entry(
    ui: &mut Context,
    entry: &TranscriptEntry,
    index: usize,
    theme: Theme,
    show_thinking: bool,
    collapse: &CollapseState,
    agent_running: bool,
) {
    match entry.role {
        TranscriptRole::User => {
            if let Some(ts) = &entry.timestamp {
                inline_line(ui, |ui| {
                    let _ = ui.text(ts).fg(theme.dim_text()).dim();
                });
            }
            render_pipe_message(ui, &entry.content, theme.user_pipe_col(), "  ");
        }
        TranscriptRole::Assistant => {
            if let Some(ts) = &entry.timestamp {
                inline_line(ui, |ui| {
                    let _ = ui.text(ts).fg(theme.dim_text()).dim();
                });
            }
            inline_line(ui, |ui| {
                let _ = ui.text("| ").fg(theme.ai_pipe_col());
            });
            render_assistant_message(ui, &entry.content, entry.is_streaming && agent_running, theme);
        }
        TranscriptRole::Thinking if show_thinking => {
            let expanded = collapse.is_expanded(index);
            let label = if expanded {
                format!("Thinking:\n{}", entry.content)
            } else {
                "Thinking…".to_string()
            };
            inline_line(ui, |ui| {
                let _ = ui.text("● ").fg(theme.dim_text());
                let _ = ui.text(label).fg(theme.muted).wrap();
            });
        }
        TranscriptRole::Thinking => {}
        TranscriptRole::Tool => {
            if let Some(tool) = entry.tool.as_ref() {
                let expanded = collapse.is_expanded(index);
                render_detail_block(ui, tool, expanded, theme, index);
            }
        }
        TranscriptRole::System => {
            inline_line(ui, |ui| {
                let _ = ui.text("> ").fg(theme.highlight());
                let _ = ui.text(&entry.content).fg(theme.dim_text()).wrap();
            });
        }
    }
}
