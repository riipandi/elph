//! Row layout and sticky-turn helpers for transcript-style scroll regions.

use std::hash::{Hash, Hasher};

use crate::text_input_layout::WrappedTextLayout;
use crate::utils::sanitize_sticky_display_text;

/// Cheap fingerprint for memoizing transcript layout across scroll-only re-renders.
pub fn transcript_messages_revision(messages: &[(&str, bool)], screen_width: u16) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    screen_width.hash(&mut hasher);
    messages.len().hash(&mut hasher);
    for (content, is_user) in messages {
        content.len().hash(&mut hasher);
        content.hash(&mut hasher);
        is_user.hash(&mut hasher);
    }
    hasher.finish()
}

/// Minimum wrapped body lines in the sticky user prompt card.
pub const STICKY_MIN_BODY_ROWS: u16 = 1;

/// Maximum wrapped body lines in the sticky user prompt card.
pub const STICKY_MAX_BODY_ROWS: u16 = 2;

/// Default wrapped body lines (alias of [`STICKY_MAX_BODY_ROWS`]).
pub const STICKY_DEFAULT_LINE_CLAMP: u16 = STICKY_MAX_BODY_ROWS;

/// Hard cap on sticky body lines (alias of [`STICKY_MAX_BODY_ROWS`]).
pub const STICKY_MAX_LINE_CLAMP: u16 = STICKY_MAX_BODY_ROWS;

/// Row span of one transcript entry inside a vertical scroll column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TranscriptRowLayout {
    pub start_row: u32,
    pub row_count: u32,
}

/// Outer bubble width (`screen_width - 3`) for transcript message chrome.
pub fn transcript_text_width(screen_width: u16) -> u16 {
    screen_width.saturating_sub(3).max(1)
}

/// Inner [`Text`] wrap width inside a transcript bubble (outer width minus horizontal padding).
pub fn transcript_bubble_inner_width(screen_width: u16, horizontal_pad_each_side: u16) -> u16 {
    transcript_text_width(screen_width)
        .saturating_sub(horizontal_pad_each_side.saturating_mul(2))
        .max(1)
}

/// Build contiguous row layouts for transcript entries separated by `gap_rows`.
pub fn layout_transcript_rows(texts: &[&str], wrap_width: u16, gap_rows: u16) -> Vec<TranscriptRowLayout> {
    let widths: Vec<u16> = texts.iter().map(|_| wrap_width).collect();
    layout_transcript_rows_widths(texts, &widths, gap_rows)
}

/// Like [`layout_transcript_rows`] with per-message inner wrap widths.
pub fn layout_transcript_rows_widths(texts: &[&str], wrap_widths: &[u16], gap_rows: u16) -> Vec<TranscriptRowLayout> {
    let mut layouts = Vec::with_capacity(texts.len());
    let mut cursor = 0u32;
    let fallback = wrap_widths.first().copied().unwrap_or(1).max(1);
    for (i, text) in texts.iter().enumerate() {
        let wrap_width = wrap_widths.get(i).copied().unwrap_or(fallback).max(1);
        let row_count = WrappedTextLayout::new_for_overlay_editor(text, wrap_width).row_count() as u32;
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

/// Wrapped row count for transcript text at `wrap_width`.
pub fn wrapped_transcript_row_count(text: &str, wrap_width: u16) -> u16 {
    WrappedTextLayout::new_for_overlay_editor(text, wrap_width).row_count()
}

/// Max body rows the panel can afford (chrome + minimum scroll area reserved).
pub fn sticky_panel_body_cap(panel_height: u16, min_scroll_rows: u16, bubble_padding_rows: u16) -> u16 {
    if panel_height <= min_scroll_rows.saturating_add(STICKY_MIN_BODY_ROWS) {
        return STICKY_MIN_BODY_ROWS;
    }
    let chrome_rows = bubble_padding_rows.saturating_add(STICKY_SCROLL_GAP_ROWS);
    let available = panel_height.saturating_sub(min_scroll_rows).saturating_sub(chrome_rows);
    if available < STICKY_MIN_BODY_ROWS {
        return STICKY_MIN_BODY_ROWS;
    }
    available.min(STICKY_MAX_BODY_ROWS)
}

/// Wrapped body line budget for sticky chrome: 1–2 rows, shrinking only on very short panels.
pub fn sticky_body_line_clamp(panel_height: u16, min_scroll_rows: u16, bubble_padding_rows: u16) -> u16 {
    sticky_panel_body_cap(panel_height, min_scroll_rows, bubble_padding_rows)
}

/// Body rows to show: natural wrapped lines, capped by panel budget (1–2).
pub fn sticky_body_line_budget(
    content: &str,
    wrap_width: u16,
    panel_height: u16,
    min_scroll_rows: u16,
    bubble_padding_rows: u16,
) -> u16 {
    let natural = wrapped_transcript_row_count(content, wrap_width);
    let panel_cap = sticky_panel_body_cap(panel_height, min_scroll_rows, bubble_padding_rows);
    natural.clamp(STICKY_MIN_BODY_ROWS, panel_cap)
}

/// Clamp transcript text to at most `max_body_lines` wrapped rows (ellipsis on last line).
pub fn clamp_wrapped_transcript_lines(text: &str, wrap_width: u16, max_body_lines: u16) -> (String, u16, bool) {
    let text = sanitize_sticky_display_text(text);
    let layout = WrappedTextLayout::new_for_overlay_editor(&text, wrap_width);
    let line_width = wrap_width.max(1) as usize;
    layout.clamp_display_lines(&text, max_body_lines, line_width)
}

/// Breathing room between the sticky card and the scrollable transcript (no border line).
pub const STICKY_SCROLL_GAP_ROWS: u16 = 1;

/// Terminal rows for sticky chrome: wrapped body + bubble padding + scroll gap.
pub fn sticky_header_display_rows(body_rows: u16, bubble_padding_rows: u16) -> u16 {
    body_rows
        .saturating_add(bubble_padding_rows)
        .saturating_add(STICKY_SCROLL_GAP_ROWS)
}

/// Resolved sticky header: line-clamped text and stable row height for viewport inset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StickyHeaderLayout {
    pub display_text: String,
    /// Wrapped body rows after content-aware sizing (1–2).
    pub body_rows: u16,
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
    let content = sanitize_sticky_display_text(content);
    let body_budget = sticky_body_line_budget(&content, wrap_width, panel_height, min_scroll_rows, bubble_padding_rows);
    let (display_text, body_rows, truncated) = clamp_wrapped_transcript_lines(&content, wrap_width, body_budget);
    let mut height = sticky_header_display_rows(body_rows, bubble_padding_rows);
    height = clamp_sticky_header_rows(height, panel_height, min_scroll_rows);
    if height == 0 {
        return None;
    }
    Some(StickyHeaderLayout {
        display_text,
        body_rows,
        height,
        truncated,
    })
}

/// Total wrapped scroll rows for a laid-out transcript (0 when empty).
pub fn transcript_content_row_count(layouts: &[TranscriptRowLayout]) -> u32 {
    layouts
        .last()
        .map(|layout| layout.start_row.saturating_add(layout.row_count))
        .unwrap_or(0)
}

/// Whether the transcript is tall enough to scroll and warrant sticky user-prompt chrome.
///
/// Empty or viewport-fitting sessions (typical at session start) return false so the sticky card
/// stays hidden until content actually overflows the panel.
pub fn transcript_supports_sticky_scroll(layouts: &[TranscriptRowLayout], viewport_rows: u16) -> bool {
    let viewport = viewport_rows.max(1) as u32;
    !layouts.is_empty() && transcript_content_row_count(layouts) > viewport
}

/// Whether the laid-out bubble at `message_idx` intersects the scroll viewport.
pub fn transcript_bubble_overlaps_viewport(
    layouts: &[TranscriptRowLayout],
    message_idx: usize,
    scroll_offset: i32,
    viewport_rows: u16,
) -> bool {
    let Some(layout) = layouts.get(message_idx) else {
        return false;
    };
    if scroll_offset < 0 {
        return false;
    }
    let view_top = scroll_offset as u32;
    let view_bottom = view_top.saturating_add(viewport_rows.max(1) as u32);
    let bubble_top = layout.start_row;
    let bubble_bottom = layout.start_row.saturating_add(layout.row_count);
    bubble_top < view_bottom && bubble_bottom > view_top
}

/// Hide the in-flow sticky source bubble when it would duplicate the pinned overlay.
pub fn sticky_source_bubble_suppressed(
    layouts: &[TranscriptRowLayout],
    sticky_idx: Option<usize>,
    scroll_offset: i32,
    viewport_rows: u16,
) -> Option<usize> {
    let idx = sticky_idx?;
    transcript_bubble_overlaps_viewport(layouts, idx, scroll_offset, viewport_rows).then_some(idx)
}

/// Index of the latest submitted user prompt eligible for sticky chrome.
pub fn latest_sticky_user_message_index(is_sticky_prompt: &[bool]) -> Option<usize> {
    is_sticky_prompt.iter().rposition(|&sticky| sticky)
}

/// Index of the submitted user prompt that should stick at the top for `scroll_offset` (lines).
///
/// `is_sticky_prompt[i]` must be true only for editor-submitted user input (not assistant, tool,
/// or plain transcript lines). Returns the last eligible entry whose start row is at or above the
/// viewport top.
pub fn sticky_user_message_index(
    layouts: &[TranscriptRowLayout],
    is_sticky_prompt: &[bool],
    scroll_offset: i32,
) -> Option<usize> {
    if layouts.len() != is_sticky_prompt.len() || scroll_offset < 0 {
        return None;
    }
    let offset = scroll_offset as u32;
    layouts
        .iter()
        .zip(is_sticky_prompt.iter())
        .enumerate()
        .rposition(|(_, (layout, sticky))| *sticky && layout.start_row <= offset)
}

/// Sticky prompt shown for the active transcript turn.
///
/// Returns `None` for an empty transcript or when content still fits the viewport (no scroll).
///
/// While `auto_scroll` is pinned to the bottom (normal follow-latest after submit), sticky is
/// **disabled** so the in-flow user bubble keeps its tinted card background. A bottom-pinned
/// offset looks like a large scroll and would otherwise pin the latest prompt to the top while
/// suppressing the source bubble (invisible / no background) in the stream.
///
/// During manual scroll: hide at the top of the transcript (`scroll_offset <= 0`) so in-flow
/// cards show without a duplicate overlay. Below that, pick the last sticky turn whose start
/// row is at or above the viewport top (the prompt you have scrolled past).
pub fn active_sticky_user_message_index(
    layouts: &[TranscriptRowLayout],
    is_sticky_prompt: &[bool],
    scroll_offset: i32,
    auto_scroll_pinned: bool,
    viewport_rows: u16,
) -> Option<usize> {
    if layouts.len() != is_sticky_prompt.len() || !transcript_supports_sticky_scroll(layouts, viewport_rows) {
        return None;
    }
    // Following the bottom: keep in-flow user cards fully painted (with bubble background).
    if auto_scroll_pinned {
        return None;
    }
    if scroll_offset <= 0 {
        return None;
    }
    sticky_user_message_index(layouts, is_sticky_prompt, scroll_offset)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sticky_panel_body_cap_respects_panel_and_chrome() {
        let pad = 2u16;
        assert_eq!(sticky_panel_body_cap(20, 3, pad), STICKY_MAX_BODY_ROWS);
        assert_eq!(sticky_panel_body_cap(8, 3, pad), STICKY_MAX_BODY_ROWS);
        assert_eq!(sticky_panel_body_cap(4, 3, pad), STICKY_MIN_BODY_ROWS);
    }

    #[test]
    fn sticky_body_line_budget_follows_natural_wrap() {
        let pad = 2u16;
        assert_eq!(sticky_body_line_budget("ok", 40, 20, 3, pad), 1);
        let long = "word ".repeat(20);
        assert_eq!(sticky_body_line_budget(long.trim(), 12, 20, 3, pad), STICKY_MAX_BODY_ROWS);
    }

    #[test]
    fn sticky_header_display_rows_includes_padding_and_scroll_gap() {
        assert_eq!(sticky_header_display_rows(2, 2), 5);
        assert_eq!(sticky_header_display_rows(1, 2), 4);
    }

    #[test]
    fn layout_sticky_header_height_tracks_content_rows() {
        let pad = 2u16;
        let short = layout_sticky_header("ok", 40, pad, 20, 3).expect("short");
        assert!(!short.truncated);
        assert_eq!(short.body_rows, 1);
        assert_eq!(short.height, sticky_header_display_rows(1, pad));

        let long = "word ".repeat(20);
        let wide = layout_sticky_header(long.trim(), 12, pad, 20, 3).expect("long");
        assert_eq!(wide.body_rows, 2);
        assert_eq!(wide.height, sticky_header_display_rows(2, pad));
    }

    #[test]
    fn clamp_wrapped_transcript_lines_wraps_before_line_clamp() {
        let long = "word ".repeat(20);
        let (text, rows, truncated) = clamp_wrapped_transcript_lines(long.trim(), 12, STICKY_MAX_BODY_ROWS);
        assert!(truncated);
        assert_eq!(rows, STICKY_MAX_BODY_ROWS);
        assert!(text.contains('\n'));
        assert!(text.contains('…'));
    }
}
