use crate::components::inline_line;
use crate::theme::Theme;
use slt::{Border, Context};

/// Todo item status for the tasks panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskStatus {
    Pending,
    InProgress,
    Done,
}

/// One task row in the TodoList panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskItem {
    pub content: String,
    pub status: TaskStatus,
}

impl TaskItem {
    pub fn marker(&self) -> &'static str {
        match self.status {
            TaskStatus::Pending => "○",
            TaskStatus::InProgress => "◐",
            TaskStatus::Done => "✓",
        }
    }
}

/// Renders the tasks panel above the input when there are active (non-done) tasks.
pub fn render_tasks_panel(ui: &mut Context, tasks: &[TaskItem], theme: Theme) {
    let active: Vec<_> = tasks.iter().filter(|t| t.status != TaskStatus::Done).collect();
    if active.is_empty() {
        return;
    }

    let _ = ui
        .bordered(Border::Rounded)
        .border_fg(theme.dim_text())
        .pt(1)
        .pb(1)
        .pl(2)
        .pr(2)
        .col(|ui| {
            let _ = ui.text("Tasks").bold().fg(theme.bright_text());
            for task in active {
                inline_line(ui, |ui| {
                    let _ = ui.text(task.marker()).fg(theme.bright_text());
                    let _ = ui.text(format!(" {}", task.content)).fg(theme.dim_text());
                });
            }
        });
}

/// System notice when all tasks complete.
pub fn format_tasks_completed_notice(tasks: &[TaskItem]) -> String {
    let mut lines = vec!["All tasks completed.".to_string()];
    for task in tasks.iter().filter(|t| t.status == TaskStatus::Done) {
        lines.push(format!("✓ {}", task.content));
    }
    lines.join("\n")
}
