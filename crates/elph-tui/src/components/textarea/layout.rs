//! Pure layout helpers for multiline prompt sizing and scroll.

use crate::text_input_layout::WrappedTextLayout;

/// Logical row count, including an empty row after a trailing `\n`.
pub fn logical_line_count(text: &str) -> u16 {
    let lines = text.chars().filter(|&c| c == '\n').count() + 1;
    lines.max(1) as u16
}

/// Display rows after soft-wrapping.
pub fn display_row_count(text: &str, viewport_width: u16) -> u16 {
    WrappedTextLayout::new_for_overlay_editor(text, viewport_width).row_count()
}

/// Cursor offset for viewport sizing (maps a single trailing `\n` to the empty continuation row).
pub fn layout_cursor_for_viewport(text: &str, cursor: usize) -> usize {
    let cursor = cursor.min(text.len());
    if !text.ends_with('\n') {
        return cursor;
    }
    if cursor == text.len() {
        return cursor;
    }
    // Only the final lone `\n` gets an empty continuation row — not blank lines in the middle.
    if cursor == text.len().saturating_sub(1) {
        let before_last = text.len().saturating_sub(1);
        if before_last == 0 || !text[..before_last].ends_with('\n') {
            return text.len();
        }
    }
    cursor
}

fn visible_row_count_from_layout(wrapped: &WrappedTextLayout, text: &str, cursor: usize) -> u16 {
    let mut rows = wrapped.row_count();
    if rows > 1 && text.ends_with('\n') {
        let (cursor_row, _) = wrapped.row_column_for_offset(text, cursor.min(text.len()));
        let last_row = rows.saturating_sub(1);
        if cursor_row < last_row {
            rows -= 1;
        }
    }
    rows.max(1)
}

/// Rows to allocate vertically: omit a trailing empty continuation row unless the cursor is on it.
pub fn visible_row_count(text: &str, cursor: usize, viewport_width: u16) -> u16 {
    let wrapped = WrappedTextLayout::new_for_overlay_editor(text, viewport_width);
    visible_row_count_from_layout(&wrapped, text, cursor)
}

pub fn compute_viewport_height(content_rows: u16, min_height: u16, max_height: Option<u16>) -> u16 {
    let min_h = min_height.max(1);
    match max_height {
        None => content_rows.max(min_h),
        Some(max) => content_rows.min(max.max(min_h)).max(min_h),
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct TextareaLayout {
    pub input_width: u16,
    pub content_rows: u16,
    pub viewport_height: u16,
    pub show_scrollbar: bool,
}

/// Viewport metrics from an existing wrap pass (cursor only affects growth-without-cap mode).
pub fn layout_metrics_from_wrapped(
    wrapped: &WrappedTextLayout,
    text: &str,
    cursor: usize,
    outer_width: u16,
    min_height: u16,
    max_height: Option<u16>,
) -> TextareaLayout {
    let scrollbar_reserved = max_height.is_some();
    let input_width = outer_width.saturating_sub(if scrollbar_reserved { 1 } else { 0 });
    let content_rows = wrapped.row_count();
    let visible_rows = match max_height {
        Some(_) => content_rows,
        None => visible_row_count_from_layout(wrapped, text, cursor),
    };
    let viewport_height = compute_viewport_height(visible_rows, min_height, max_height);
    let show_scrollbar = scrollbar_reserved;
    TextareaLayout {
        input_width,
        content_rows,
        viewport_height,
        show_scrollbar,
    }
}

/// Layout metrics plus a single shared wrap pass for cursor/scroll rendering.
pub fn layout_textarea_measured(
    text: &str,
    cursor: usize,
    outer_width: u16,
    min_height: u16,
    max_height: Option<u16>,
) -> (TextareaLayout, WrappedTextLayout) {
    let scrollbar_reserved = max_height.is_some();
    let input_width = outer_width.saturating_sub(if scrollbar_reserved { 1 } else { 0 });
    let wrapped = WrappedTextLayout::new_for_overlay_editor(text, input_width);
    let layout = layout_metrics_from_wrapped(&wrapped, text, cursor, outer_width, min_height, max_height);
    (layout, wrapped)
}

pub fn layout_textarea(
    text: &str,
    cursor: usize,
    outer_width: u16,
    min_height: u16,
    max_height: Option<u16>,
) -> TextareaLayout {
    layout_textarea_measured(text, cursor, outer_width, min_height, max_height).0
}
