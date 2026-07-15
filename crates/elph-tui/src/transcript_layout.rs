//! Row layout and sticky-turn helpers for transcript-style scroll regions.

use crate::text_input_layout::{WrappedTextLayout, text_input_wrap_width};

/// Default wrapped body lines shown in a sticky user prompt (CSS `line-clamp` analogue).
pub const STICKY_DEFAULT_LINE_CLAMP: u16 = 4;

/// Hard cap on sticky body lines even on tall terminals.
pub const STICKY_MAX_LINE_CLAMP: u16 = 6;

/// Row span of one transcript entry inside a vertical scroll column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TranscriptRowLayout {
    pub start_row: u32,
    pub row_count: u32,
}

/// Line-wrap width for transcript body text (matches `screen_width - 3` bubbles).
pub fn transcript_text_width(screen_width: u16) -> u16 {
    screen_width.saturating_sub(3).max(1)
}

/// Build contiguous row layouts for transcript entries separated by `gap_rows`.
pub fn layout_transcript_rows(texts: &[&str], wrap_width: u16, gap_rows: u16) -> Vec<TranscriptRowLayout> {
    let mut layouts = Vec::with_capacity(texts.len());
    let mut cursor = 0u32;
    for (i, text) in texts.iter().enumerate() {
        let row_count = WrappedTextLayout::new(text, wrap_width).row_count() as u32;
        layouts.push(TranscriptRowLayout {
            start_row: cursor,
            row_count,
        });
        cursor += row_count;
        if i + 1 < texts.len() {
            cursor += gap_rows as u32;
        }
    }
    layouts
}

/// Visible scroll viewport after reserving `sticky_header_rows` at the top.
pub fn scroll_viewport_height(viewport_height: u16, sticky_header_rows: u16) -> u16 {
    viewport_height.saturating_sub(sticky_header_rows).max(1)
}

/// Row span of a sticky transcript header (wrapped body + bubble padding).
pub fn sticky_header_row_count(layout: &TranscriptRowLayout, bubble_padding_rows: u16) -> u16 {
    layout
        .row_count
        .saturating_add(bubble_padding_rows as u32)
        .min(u16::MAX as u32) as u16
}

/// Cap sticky header height so at least `min_scroll_rows` remain scrollable.
pub fn clamp_sticky_header_rows(sticky_rows: u16, viewport_height: u16, min_scroll_rows: u16) -> u16 {
    if viewport_height <= min_scroll_rows {
        return 0;
    }
    sticky_rows.min(viewport_height.saturating_sub(min_scroll_rows))
}

/// Wrapped body line budget for sticky chrome given the full panel height.
pub fn sticky_body_line_clamp(panel_height: u16, min_scroll_rows: u16) -> u16 {
    if panel_height <= min_scroll_rows.saturating_add(1) {
        return 1;
    }
    let scroll_budget = panel_height.saturating_sub(min_scroll_rows).saturating_sub(1);
    STICKY_DEFAULT_LINE_CLAMP
        .min(scroll_budget)
        .clamp(1, STICKY_MAX_LINE_CLAMP)
}

/// Clamp transcript text to at most `max_body_lines` wrapped rows (ellipsis on last line).
pub fn clamp_wrapped_transcript_lines(text: &str, wrap_width: u16, max_body_lines: u16) -> (String, u16, bool) {
    let layout = WrappedTextLayout::new(text, wrap_width);
    layout.clamp_display_lines(text, max_body_lines, text_input_wrap_width(wrap_width))
}

/// Terminal rows for sticky chrome: clamped body + bubble padding + optional hint row.
pub fn sticky_header_display_rows(body_rows: u16, bubble_padding_rows: u16, truncated: bool) -> u16 {
    let mut rows = body_rows.saturating_add(bubble_padding_rows);
    if truncated {
        rows = rows.saturating_add(1);
    }
    rows
}

/// Resolved sticky header: line-clamped text and stable row height for viewport inset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StickyHeaderLayout {
    pub display_text: String,
    pub height: u16,
    pub truncated: bool,
}

/// Build sticky header layout for one user message inside the transcript panel.
pub fn layout_sticky_header(
    content: &str,
    wrap_width: u16,
    bubble_padding_rows: u16,
    panel_height: u16,
    min_scroll_rows: u16,
) -> Option<StickyHeaderLayout> {
    let body_clamp = sticky_body_line_clamp(panel_height, min_scroll_rows);
    let (display_text, body_rows, truncated) = clamp_wrapped_transcript_lines(content, wrap_width, body_clamp);
    let mut height = sticky_header_display_rows(body_rows, bubble_padding_rows, truncated);
    height = clamp_sticky_header_rows(height, panel_height, min_scroll_rows);
    if height == 0 {
        return None;
    }
    Some(StickyHeaderLayout {
        display_text,
        height,
        truncated,
    })
}

/// Index of the user message that should stick at the top for `scroll_offset` (lines).
///
/// Returns the last user entry whose start row is at or above the viewport top.
pub fn sticky_user_message_index(
    layouts: &[TranscriptRowLayout],
    is_user: &[bool],
    scroll_offset: i32,
) -> Option<usize> {
    if layouts.len() != is_user.len() || scroll_offset <= 0 {
        return None;
    }
    let offset = scroll_offset as u32;
    layouts
        .iter()
        .zip(is_user.iter())
        .enumerate()
        .rposition(|(_, (layout, user))| *user && layout.start_row <= offset)
}

/// Sticky prompt for manual scroll only — not while `auto_scroll` is pinned to the bottom.
///
/// When pinned, [`effective_scroll_offset`] equals the bottom offset (often large). Feeding that
/// into [`sticky_user_message_index`] wrongly pins the latest user bubble after submit and causes
/// viewport inset flicker on long pasted messages.
pub fn active_sticky_user_message_index(
    layouts: &[TranscriptRowLayout],
    is_user: &[bool],
    scroll_offset: i32,
    auto_scroll_pinned: bool,
) -> Option<usize> {
    if auto_scroll_pinned {
        return None;
    }
    sticky_user_message_index(layouts, is_user, scroll_offset)
}

/// Effective scroll offset when `auto_scroll` may be pinned to the bottom.
pub fn effective_scroll_offset(
    scroll_offset: i32,
    auto_scroll_pinned: bool,
    content_height: u16,
    viewport_height: u16,
) -> i32 {
    if auto_scroll_pinned {
        crate::components::scroll_view_max_offset(content_height, viewport_height)
    } else {
        scroll_offset
    }
}
