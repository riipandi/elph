use crate::components::text_optional_color;
use crate::theme::Theme;
use crate::transcript::{ToolExecutionState, ToolExecutionStatus};
use crate::utils::strip_ansi;
use slt::{Border, Context};

/// Renders a single tool execution card.
pub fn render_tool_execution_card(ui: &mut Context, tool: &ToolExecutionState, theme: Theme, compact: bool) {
    let status = status_label(tool.status);
    let output = if tool.output.is_empty() || compact {
        String::new()
    } else {
        strip_ansi(&tool.output)
    };

    let _ = ui
        .bordered(Border::Single)
        .border_fg(theme.frame_border)
        .p(1)
        .mb(1)
        .grow(1)
        .col(|ui| {
            ui.text(format!("⚙ {}  [{status}]", tool.name));
            if !tool.args_summary.is_empty() {
                text_optional_color(ui, &tool.args_summary, Some(theme.muted));
            }
            if tool.status == ToolExecutionStatus::Running {
                ui.text("⠋ Running... (Ctrl+C to cancel)");
            }
            if !output.is_empty() {
                let _ = ui.markdown(&output);
            }
            if tool.requires_approval && tool.status == ToolExecutionStatus::Pending {
                let _ = ui.container().gap(2).row(|ui| {
                    ui.text("[Approve]");
                    ui.text("[Deny]");
                });
            }
        });
}

/// Renders a vertical list of tool execution cards.
pub fn render_tool_execution_list(ui: &mut Context, tools: &[ToolExecutionState], theme: Theme) {
    let _ = ui.container().grow(1).col(|ui| {
        for tool in tools {
            render_tool_execution_card(ui, tool, theme, false);
        }
    });
}

fn status_label(status: ToolExecutionStatus) -> &'static str {
    match status {
        ToolExecutionStatus::Pending => "pending",
        ToolExecutionStatus::Running => "running",
        ToolExecutionStatus::Success => "ok",
        ToolExecutionStatus::Error => "error",
        ToolExecutionStatus::Cancelled => "cancelled",
    }
}
