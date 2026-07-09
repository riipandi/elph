use crate::theme::Theme;
use crate::utils::strip_ansi;
use slt::Context;

fn streaming_cursor_visible() -> bool {
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    (ms / 400).is_multiple_of(2)
}

/// Renders an assistant message with markdown formatting and optional streaming cursor.
pub fn render_assistant_message(ui: &mut Context, content: &str, is_streaming: bool, theme: Theme) {
    let content = strip_ansi(content);
    if content.trim().is_empty() && !is_streaming {
        return;
    }

    if !content.trim().is_empty() {
        let _ = ui.markdown(&content);
    }

    if is_streaming && streaming_cursor_visible() {
        let _ = ui.text("▌").fg(theme.highlight());
    }
}
