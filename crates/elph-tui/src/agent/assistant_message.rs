use crate::diff::partition_streaming_markdown;
use crate::theme::Theme;
use crate::utils::strip_ansi;
use slt::Context;

/// Renders an assistant message with markdown formatting and an optional streaming cursor.
pub fn render_assistant_message(ui: &mut Context, content: &str, show_streaming_cursor: bool, theme: Theme) {
    let content = strip_ansi(content);
    if content.trim().is_empty() && !show_streaming_cursor {
        return;
    }

    let streaming = show_streaming_cursor;
    let (stable, tail) = partition_streaming_markdown(&content, streaming);

    if !stable.is_empty() {
        let _ = ui.markdown(stable.trim_end());
    }

    if !tail.is_empty() {
        let _ = ui.text(tail).wrap();
    }

    if show_streaming_cursor {
        let _ = ui.text("▌").fg(theme.highlight());
    }
}
