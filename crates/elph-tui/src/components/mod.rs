mod inline;
mod label;

pub use inline::{inline_label_value, inline_line};
pub use label::render_label;

use slt::{Border, Color, Context};

/// Renders text with an optional foreground color.
pub fn text_optional_color(ui: &mut Context, content: impl AsRef<str>, color: Option<Color>) {
    let content = content.as_ref();
    if let Some(c) = color.filter(|c| *c != Color::Reset) {
        ui.text(content).fg(c);
    } else {
        ui.text(content);
    }
}

/// Renders a rounded frame around nested content.
pub fn frame(ui: &mut Context, theme: crate::theme::Theme, f: impl FnOnce(&mut Context)) {
    let _ = ui
        .bordered(Border::Rounded)
        .border_fg(theme.frame_border)
        .pt(2)
        .pb(2)
        .pl(8)
        .pr(8)
        .col(f);
}
