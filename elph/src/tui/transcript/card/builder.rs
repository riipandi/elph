//! Build scroll-view bubbles from transcript messages.

use elph_tui::TranscriptRowLayout;
use iocraft::prelude::*;

use super::super::types::{TranscriptMessage, TranscriptStyle};
use super::kinds::{
    chat_response_card, error_card, meta_card, skill_prompt_card, status_line_card, suppressed_sticky_user_prompt_card,
    thinking_card, thinking_response_pair_card, tool_call_card, user_prompt_card,
};
use super::toggle_ctx::CollapsibleToggleCtx;

/// Extra rows above/below the viewport when windowing transcript cards.
const WINDOW_OVERSCAN_ROWS: u32 = 24;
/// Always keep at least this many trailing messages fully mounted (streaming tail).
const WINDOW_MIN_TAIL_MESSAGES: usize = 12;

pub fn build_transcript_bubbles(
    screen_width: u16,
    messages: &[TranscriptMessage],
    suppress_sticky_source: Option<usize>,
    toggle: Option<CollapsibleToggleCtx>,
) -> Vec<AnyElement<'static>> {
    build_transcript_bubbles_range(screen_width, messages, 0, messages.len(), suppress_sticky_source, toggle)
}

/// Windowed bubble list: off-screen runs become fixed-height spacers so scroll metrics
/// stay correct while the element tree stays O(viewport) instead of O(history).
pub fn build_transcript_bubbles_windowed(
    screen_width: u16,
    messages: &[TranscriptMessage],
    row_layouts: &[TranscriptRowLayout],
    view_start_row: u32,
    view_rows: u32,
    suppress_sticky_source: Option<usize>,
    toggle: Option<CollapsibleToggleCtx>,
) -> Vec<AnyElement<'static>> {
    if messages.is_empty() {
        return Vec::new();
    }
    if row_layouts.len() != messages.len() {
        // Layout cache miss / mismatch — fall back to full rebuild.
        return build_transcript_bubbles(screen_width, messages, suppress_sticky_source, toggle);
    }

    let total_rows = row_layouts
        .last()
        .map(|layout| layout.start_row.saturating_add(layout.row_count))
        .unwrap_or(0);
    let view_end_row = view_start_row
        .saturating_add(view_rows)
        .saturating_add(WINDOW_OVERSCAN_ROWS)
        .min(total_rows);
    let view_start_row = view_start_row.saturating_sub(WINDOW_OVERSCAN_ROWS);

    // Prefer mounting the live tail so streaming cards stay interactive.
    let tail_start = messages.len().saturating_sub(WINDOW_MIN_TAIL_MESSAGES);

    let mut first_visible = messages.len();
    let mut last_visible = 0usize;
    for (index, layout) in row_layouts.iter().enumerate() {
        let msg_end = layout.start_row.saturating_add(layout.row_count);
        let intersects = msg_end > view_start_row && layout.start_row < view_end_row;
        let in_tail = index >= tail_start;
        if intersects || in_tail {
            first_visible = first_visible.min(index);
            last_visible = last_visible.max(index);
        }
    }

    if first_visible > last_visible {
        // Nothing intersects — keep the tail mounted.
        first_visible = tail_start;
        last_visible = messages.len().saturating_sub(1);
    }

    // Expand to whole flush pairs so thinking+response stay together.
    while first_visible > 0
        && messages[first_visible.saturating_sub(1)]
            .style
            .forms_flush_pair_with(messages[first_visible].style)
    {
        first_visible -= 1;
    }
    while last_visible + 1 < messages.len()
        && messages[last_visible]
            .style
            .forms_flush_pair_with(messages[last_visible + 1].style)
    {
        last_visible += 1;
    }

    let mut bubbles = Vec::with_capacity((last_visible - first_visible + 1).saturating_add(2));

    if first_visible > 0 {
        let spacer_rows = row_layouts
            .get(first_visible)
            .map(|layout| layout.start_row)
            .unwrap_or(0);
        push_transcript_spacers(&mut bubbles, spacer_rows);
    }

    bubbles.extend(build_transcript_bubbles_range(
        screen_width,
        messages,
        first_visible,
        last_visible.saturating_add(1),
        suppress_sticky_source,
        toggle,
    ));

    if last_visible + 1 < messages.len() {
        // Include inter-message gap after the last visible row (encoded in next start_row).
        let after_start = row_layouts
            .get(last_visible + 1)
            .map(|layout| layout.start_row)
            .unwrap_or_else(|| {
                row_layouts
                    .get(last_visible)
                    .map(|layout| layout.start_row.saturating_add(layout.row_count))
                    .unwrap_or(0)
            });
        let spacer_rows = total_rows.saturating_sub(after_start);
        push_transcript_spacers(&mut bubbles, spacer_rows);
    }

    bubbles
}

/// Cap per spacer view — very large single heights stress iocraft layout.
const SPACER_CHUNK_ROWS: u32 = 4_096;

fn push_transcript_spacers(bubbles: &mut Vec<AnyElement<'static>>, rows: u32) {
    let mut remaining = rows;
    while remaining > 0 {
        let chunk = remaining.min(SPACER_CHUNK_ROWS);
        bubbles.push(transcript_spacer(chunk));
        remaining = remaining.saturating_sub(chunk);
    }
}

fn transcript_spacer(rows: u32) -> AnyElement<'static> {
    let height = rows.min(u16::MAX as u32) as u16;
    if height == 0 {
        return element!(View).into();
    }
    element! {
        View(
            width: 100pct,
            height: height,
            flex_shrink: 0f32,
            flex_grow: 0f32,
        )
    }
    .into()
}

fn build_transcript_bubbles_range(
    screen_width: u16,
    messages: &[TranscriptMessage],
    start: usize,
    end: usize,
    suppress_sticky_source: Option<usize>,
    toggle: Option<CollapsibleToggleCtx>,
) -> Vec<AnyElement<'static>> {
    let end = end.min(messages.len());
    let start = start.min(end);
    let mut bubbles = Vec::with_capacity(end.saturating_sub(start));
    let mut index = start;
    while index < end {
        let message = &messages[index];
        if let Some(next) = messages.get(index + 1)
            && index + 1 < end
            && message.style.forms_flush_pair_with(next.style)
        {
            let pair_last = next;
            let margin_bottom = pair_last.transcript_margin_bottom(messages.get(index + 2));
            bubbles.push(thinking_response_pair_card(
                screen_width,
                message,
                next,
                index,
                margin_bottom,
                toggle,
            ));
            index += 2;
            continue;
        }
        let margin_bottom = message.transcript_margin_bottom(messages.get(index + 1));
        bubbles.push(transcript_message_bubble(
            screen_width,
            message,
            index,
            margin_bottom,
            suppress_sticky_source == Some(index),
            toggle,
        ));
        index += 1;
    }
    bubbles
}

pub fn transcript_message_bubble(
    screen_width: u16,
    message: &TranscriptMessage,
    message_index: usize,
    margin_bottom: u16,
    suppress_sticky_source: bool,
    toggle: Option<CollapsibleToggleCtx>,
) -> AnyElement<'static> {
    match message.style {
        TranscriptStyle::User if suppress_sticky_source => {
            suppressed_sticky_user_prompt_card(screen_width, message, margin_bottom)
        }
        TranscriptStyle::User => user_prompt_card(screen_width, message, margin_bottom),
        TranscriptStyle::SkillPrompt => skill_prompt_card(screen_width, message, margin_bottom),
        TranscriptStyle::Thinking => thinking_card(screen_width, message, margin_bottom, message_index, toggle),
        TranscriptStyle::Assistant => chat_response_card(screen_width, message, margin_bottom, message_index, toggle),
        TranscriptStyle::ToolRunning | TranscriptStyle::ToolSuccess | TranscriptStyle::ToolFailed => {
            tool_call_card(screen_width, message, margin_bottom, message_index, toggle)
        }
        TranscriptStyle::Error => error_card(screen_width, message, margin_bottom),
        TranscriptStyle::Meta => meta_card(screen_width, message, margin_bottom),
        TranscriptStyle::StatusRunning | TranscriptStyle::StatusSuccess | TranscriptStyle::StatusFailed => {
            status_line_card(screen_width, message, margin_bottom)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::transcript::layout::layout_transcript_rows;
    use crate::tui::transcript::types::{TranscriptMessage, TranscriptStyle};

    #[test]
    fn windowed_build_emits_spacers_for_long_history() {
        let mut messages = Vec::new();
        for i in 0..40 {
            messages.push(TranscriptMessage::text(
                format!("status line {i}"),
                TranscriptStyle::StatusSuccess,
            ));
        }
        // Live streaming tail.
        messages.push(TranscriptMessage::tool_call(
            "wait_agent",
            r#"{"agent_id":"x"}"#,
            TranscriptStyle::ToolRunning,
        ));

        let layouts = layout_transcript_rows(&messages, 80);
        let total_rows = layouts
            .last()
            .map(|l| l.start_row.saturating_add(l.row_count))
            .unwrap_or(0);
        // View only the bottom of the transcript.
        let view_start = total_rows.saturating_sub(8);
        let bubbles = build_transcript_bubbles_windowed(80, &messages, &layouts, view_start, 8, None, None);
        // Full rebuild would be 41 bubbles; windowed should be much smaller.
        assert!(
            bubbles.len() < messages.len(),
            "expected windowing, got {} bubbles for {} messages",
            bubbles.len(),
            messages.len()
        );
        assert!(!bubbles.is_empty());
    }
}
