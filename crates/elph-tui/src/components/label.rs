use slt::{Color, Context};

/// Renders a single text label with an optional foreground color.
pub fn render_label(ui: &mut Context, content: &str, color: Option<Color>) {
    super::text_optional_color(ui, content, color);
}
