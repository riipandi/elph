use crate::components::inline_line;
use crate::theme::Theme;
use crate::transcript::{ToolExecutionState, ToolExecutionStatus};
use crate::utils::strip_ansi;
use slt::{Color, Context};

/// Tracks which transcript detail blocks are expanded (by entry index).
#[derive(Debug, Clone, Default)]
pub struct CollapseState {
    expanded: Vec<usize>,
}

impl CollapseState {
    pub fn is_expanded(&self, index: usize) -> bool {
        self.expanded.contains(&index)
    }

    pub fn toggle(&mut self, index: usize) {
        if let Some(pos) = self.expanded.iter().position(|&i| i == index) {
            self.expanded.remove(pos);
        } else {
            self.expanded.push(index);
        }
    }

    pub fn toggle_newest(&mut self, entries_len: usize) {
        if entries_len > 0 {
            self.toggle(entries_len - 1);
        }
    }
}

/// Dot glyph for collapsible detail blocks (see `docs/tui.md`).
pub fn detail_dot(status: ToolExecutionStatus, running: bool) -> char {
    match status {
        ToolExecutionStatus::Running | ToolExecutionStatus::Pending if running => '○',
        _ => '●',
    }
}

/// Color for the detail dot indicator.
pub fn detail_dot_color(status: ToolExecutionStatus, theme: Theme) -> Color {
    match status {
        ToolExecutionStatus::Success => Color::Green,
        ToolExecutionStatus::Error => Color::Red,
        ToolExecutionStatus::Cancelled => theme.yellow_col(),
        ToolExecutionStatus::Running => Color::Yellow,
        ToolExecutionStatus::Pending => theme.dim_text(),
    }
}

/// Compact single-line tool indicator: `● Bash(echo hi) (ctrl+o to expand)`.
pub fn format_detail_hint(tool: &ToolExecutionState) -> String {
    let args = tool.args_summary.trim();
    if args.is_empty() {
        format!("{}({})", tool.name, tool.name)
    } else {
        let short = if args.chars().count() > 40 {
            format!("{}…", args.chars().take(37).collect::<String>())
        } else {
            args.to_string()
        };
        format!("{}({short})", tool.name)
    }
}

/// Renders a collapsible tool/detail block.
pub fn render_detail_block(ui: &mut Context, tool: &ToolExecutionState, expanded: bool, theme: Theme, index: usize) {
    let dot = detail_dot(tool.status, tool.status == ToolExecutionStatus::Running);
    let dot_color = detail_dot_color(tool.status, theme);
    let hint = format_detail_hint(tool);
    let suffix = if expanded {
        " (ctrl+o to collapse)"
    } else {
        " (ctrl+o to expand)"
    };

    inline_line(ui, |ui| {
        let _ = ui.text(format!("{dot} ")).fg(dot_color);
        let _ = ui.text(hint).fg(theme.bright_text());
        let _ = ui.text(suffix).fg(theme.dim_text()).dim();
        let _ = ui.spacer();
        let _ = ui.text(format!("#{index}")).fg(theme.dim_text()).dim();
    });

    if expanded && !tool.output.is_empty() {
        let output = strip_ansi(&tool.output);
        let _ = ui.container().pt(1).pb(1).pl(2).pr(2).col(|ui| {
            let _ = ui.text(&output).dim().wrap();
        });
    }
}

/// Renders a pipe-column user or assistant message.
pub fn render_pipe_message(ui: &mut Context, content: &str, pipe_color: Color, continuation_indent: &str) {
    let trimmed = content.trim_end();
    let mut lines = trimmed.lines();
    let Some(first) = lines.next() else {
        return;
    };
    inline_line(ui, |ui| {
        let _ = ui.text("| ").fg(pipe_color);
        let _ = ui.text(first).wrap();
    });
    for line in lines {
        inline_line(ui, |ui| {
            let _ = ui.text(continuation_indent);
            let _ = ui.text(line).wrap();
        });
    }
}
