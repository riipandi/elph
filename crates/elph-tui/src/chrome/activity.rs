use std::time::Instant;

use crate::components::inline_line;
use crate::theme::Theme;
use slt::{Context, widgets::SpinnerState};

/// Activity line shown between chat and input while the agent is busy.
#[derive(Debug, Clone, Default)]
pub struct ActivityState {
    pub label: String,
    pub started: Option<Instant>,
    pub visible: bool,
    pub cancel_requested: bool,
}

impl ActivityState {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            started: Some(Instant::now()),
            visible: true,
            cancel_requested: false,
        }
    }

    pub fn working() -> Self {
        Self::new("Working")
    }

    pub fn responding() -> Self {
        Self::new("Responding")
    }

    pub fn running_tool(name: &str, args: &str) -> Self {
        let detail = if args.is_empty() {
            name.to_string()
        } else {
            format!("{name}({args})")
        };
        Self::new(format!("Running {detail}"))
    }

    pub fn clear(&mut self) {
        self.visible = false;
        self.started = None;
        self.cancel_requested = false;
    }

    pub fn elapsed_secs(&self) -> f64 {
        self.started.map(|t| t.elapsed().as_secs_f64()).unwrap_or(0.0)
    }

    pub fn request_cancel(&mut self) {
        self.cancel_requested = true;
    }
}

/// Renders activity between chat and input (`Responding... 0.2s` or legacy `Working · N.Ns`).
pub fn render_activity(ui: &mut Context, state: &ActivityState, theme: Theme, spinner: &SpinnerState) {
    if !state.visible {
        return;
    }
    let elapsed = state.elapsed_secs();
    let composer_style = state.label == "Responding";
    inline_line(ui, |ui| {
        let _ = ui.spinner(spinner);
        if composer_style {
            let _ = ui.text(format!(" Responding... {:.1}s", elapsed)).fg(theme.dim_text());
            if state.cancel_requested {
                let _ = ui.text(" (cancelling)").fg(theme.yellow_col());
            } else {
                let _ = ui.spacer();
                let _ = ui
                    .text("Enter queue · Ctrl+Enter steer · Ctrl+C cancel")
                    .fg(theme.dim_text())
                    .dim();
            }
        } else {
            let _ = ui
                .text(format!(" {} · {:.1}s", state.label, elapsed))
                .fg(theme.dim_text());
            if state.cancel_requested {
                let _ = ui.text(" (cancelling)").fg(theme.yellow_col());
            } else {
                let _ = ui.spacer();
                let _ = ui
                    .text("Enter queue · Ctrl+Enter steer · Ctrl+C cancel")
                    .fg(theme.dim_text())
                    .dim();
            }
        }
    });
}
