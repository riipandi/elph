//! Row layout and sticky-turn helpers for transcript-style scroll regions.

use crate::text_input_layout::WrappedTextLayout;

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
        .filter(|(_, (layout, user))| **user && layout.start_row <= offset)
        .map(|(i, _)| i)
        .last()
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
