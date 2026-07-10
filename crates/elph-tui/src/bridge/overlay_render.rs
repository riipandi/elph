//! SLT render helper for diff overlay stacks.

use crate::utils::strip_ansi;
use slt::{Align, Color, Context, Justify};

use super::overlay_state::OverlayStack;

/// Renders the diff overlay stack as a full-screen or compact SLT overlay.
pub fn render_diff_overlay(
    ui: &mut Context,
    stack: &mut OverlayStack,
    term_width: u32,
    term_height: u32,
    visible: bool,
    compact: bool,
    show_backdrop: bool,
) {
    if !visible {
        return;
    }

    let width = term_width.max(1) as u16;
    let height = term_height.max(1) as u16;
    let lines = if compact {
        stack.render_focused(width, height)
    } else {
        stack.render(width, height)
    };

    if lines.is_empty() {
        return;
    }

    let content = strip_ansi(&lines.join("\n"));
    let mut builder = ui.container().grow(1).justify(Justify::Center).align(Align::Center);
    if show_backdrop {
        builder = builder.bg(Color::DarkGray);
    }
    let _ = builder.col(|ui| {
        ui.text(content);
    });
}
