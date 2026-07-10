use slt::{Color, Context};

/// Renders mixed-style segments on one logical line (SLT `gap = 0`).
///
/// Prefer this over [`Context::row`] when composing label/value pairs or
/// prefix + content inside a bordered panel — see SLT `ContainerBuilder::line`.
pub fn inline_line(ui: &mut Context, f: impl FnOnce(&mut Context)) {
    let _ = ui.line(f);
}

/// Inline `label` + `value` with distinct colors, no gap between segments.
pub fn inline_label_value(ui: &mut Context, label: &str, value: &str, label_color: Color, value_color: Color) {
    inline_line(ui, |ui| {
        let _ = ui.text(label).fg(label_color);
        let _ = ui.text(value).fg(value_color);
    });
}
