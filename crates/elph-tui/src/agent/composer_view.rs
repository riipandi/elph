use super::assistant_message::render_assistant_message;
use super::detail_block::CollapseState;
use crate::components::inline_line;
use crate::shell::shell_panel_pad;
use crate::theme::Theme;
use crate::transcript::{ToolExecutionState, ToolExecutionStatus, TranscriptEntry, TranscriptRole};
use crate::utils::strip_ansi;
use slt::{Border, Color, Context};

const BLOCK_GAP_LINES: u32 = 1;
const USER_PREFIX: &str = "› ";
const DIAMOND: &str = "♦ ";

/// Composer-style transcript (Cursor agent layout).
pub fn render_composer_transcript(
    ui: &mut Context,
    entries: &[TranscriptEntry],
    show_thinking: bool,
    theme: Theme,
    collapse: &CollapseState,
) {
    let gap = ui.spacing().xs();
    let _ = ui.container().gap(gap).col(|ui| {
        let mut prev_role: Option<TranscriptRole> = None;
        for (index, entry) in entries.iter().enumerate() {
            if prev_role.is_some() && needs_gap(prev_role, entry.role) {
                for _ in 0..BLOCK_GAP_LINES {
                    let _ = ui.text("");
                }
            }
            render_entry(ui, entry, index, theme, show_thinking, collapse);
            prev_role = Some(entry.role);
        }
    });
}

fn needs_gap(prev: Option<TranscriptRole>, current: TranscriptRole) -> bool {
    prev.is_some_and(|p| p != current || matches!(current, TranscriptRole::User))
}

fn render_entry(
    ui: &mut Context,
    entry: &TranscriptEntry,
    index: usize,
    theme: Theme,
    show_thinking: bool,
    collapse: &CollapseState,
) {
    match entry.role {
        TranscriptRole::User => render_user_card(ui, &entry.content, theme),
        TranscriptRole::Assistant => {
            render_assistant_message(ui, &entry.content, entry.is_streaming, theme);
        }
        TranscriptRole::Thinking if show_thinking => {
            render_thought_block(ui, entry, collapse.is_expanded(index), theme);
        }
        TranscriptRole::Thinking => {}
        TranscriptRole::Tool => {
            if let Some(tool) = entry.tool.as_ref() {
                render_tool_block(ui, tool, collapse.is_expanded(index), theme);
            }
        }
        TranscriptRole::System => {
            inline_line(ui, |ui| {
                let _ = ui.text(DIAMOND).fg(theme.dim_text());
                let _ = ui.text(&entry.content).dim();
            });
        }
    }
}

/// Bordered user turn — `› message`.
pub fn render_user_card(ui: &mut Context, content: &str, theme: Theme) {
    let pad = shell_panel_pad(ui);
    let text = content.trim();
    if text.is_empty() {
        return;
    }
    let _ = ui
        .bordered(Border::Rounded)
        .border_fg(theme.dim_text())
        .p(pad)
        .col(|ui| {
            for line in text.lines() {
                inline_line(ui, |ui| {
                    let _ = ui.text(USER_PREFIX).fg(theme.highlight());
                    let _ = ui.text(line);
                });
            }
        });
}

fn render_thought_block(ui: &mut Context, entry: &TranscriptEntry, expanded: bool, theme: Theme) {
    let duration = entry.timestamp.as_deref().map(|_| "0.2").unwrap_or("…");
    let header = format!("Thought for {duration}s");

    inline_line(ui, |ui| {
        let _ = ui.text(DIAMOND).fg(theme.dim_text());
        let _ = ui.text(header).fg(theme.dim_text()).italic();
    });

    if expanded && !entry.content.trim().is_empty() {
        let _ = ui.container().pl(2).col(|ui| {
            for line in entry.content.lines() {
                let _ = ui.text(line).dim();
            }
        });
    }
}

fn tool_block_label(tool: &ToolExecutionState) -> &'static str {
    let name = tool.name.to_ascii_lowercase();
    if name.contains("edit") || name.contains("write") || name.contains("str_replace") || name.contains("patch") {
        "Edit"
    } else if name.contains("shell") || name.contains("bash") || name.contains("run") || name.contains("command") {
        "Run"
    } else {
        "Tool"
    }
}

/// `♦ Edit  /path/to/file` with optional expanded body.
pub fn render_tool_block(ui: &mut Context, tool: &ToolExecutionState, expanded: bool, theme: Theme) {
    let label = tool_block_label(tool);
    let detail = tool_detail_line(tool);

    inline_line(ui, |ui| {
        let _ = ui.text(DIAMOND).fg(tool_status_color(tool.status, theme));
        let _ = ui.text(format!("{label}  ")).fg(theme.dim_text());
        if !detail.is_empty() {
            let _ = ui.text(detail).fg(Color::Yellow);
        }
    });

    if expanded {
        let body = tool_body(tool);
        if !body.is_empty() {
            let pad = shell_panel_pad(ui);
            let _ = ui.container().pl(2).pt(1).col(|ui| {
                let _ = ui
                    .bordered(Border::Single)
                    .border_fg(theme.dim_text())
                    .p(pad)
                    .col(|ui| {
                        for line in body.lines() {
                            let _ = ui.text(line);
                        }
                    });
            });
        }
    }
}

fn tool_detail_line(tool: &ToolExecutionState) -> String {
    let args = tool.args_summary.trim();
    if args.is_empty() {
        return String::new();
    }
    if args.contains('/') || args.contains('.') {
        args.lines().next().unwrap_or(args).to_string()
    } else if args.chars().count() > 72 {
        format!("{}…", args.chars().take(69).collect::<String>())
    } else {
        args.to_string()
    }
}

fn tool_body(tool: &ToolExecutionState) -> String {
    if !tool.output.trim().is_empty() {
        strip_ansi(&tool.output)
    } else {
        tool.args_summary.trim().to_string()
    }
}

/// Sample transcript for Composer layout previews and empty-state demos.
pub fn composer_demo_entries() -> Vec<TranscriptEntry> {
    vec![
        TranscriptEntry::user(
            "Lakukan perbaikan dan improvement menyeluruh: pastikan seluruh implementasi TUI \
             mengikuti best practice dari SuperLightTUI.",
        ),
        TranscriptEntry::thinking("Menganalisis layout shell dan pola spacing dari cookbook.", false),
        TranscriptEntry::tool(
            ToolExecutionState::new("1", "edit")
                .with_args("/crates/elph-tui/src/shell/layout.rs")
                .with_status(ToolExecutionStatus::Success)
                .with_output(
                    "#[derive(Debug, Clone)]\npub struct ShellChrome<'a> {\n    pub tier: ShellTier,\n    ...\n}",
                ),
        ),
        TranscriptEntry::assistant(
            "Perbaikan menyeluruh TUI mengikuti pola SuperLightTUI sudah diterapkan.\n\n\
             - Modul `shell` baru dengan layout bersama\n\
             - User message dalam bordered card\n\
             - Blok agent: ♦ Thought, ♦ Edit, ♦ Run\n\
             - Status bar kompak + prompt `›`",
        ),
    ]
}

fn tool_status_color(status: ToolExecutionStatus, theme: Theme) -> Color {
    match status {
        ToolExecutionStatus::Success => Color::Green,
        ToolExecutionStatus::Error => Color::Red,
        ToolExecutionStatus::Cancelled => theme.yellow_col(),
        ToolExecutionStatus::Running | ToolExecutionStatus::Pending => Color::Yellow,
    }
}
