//! Build scroll-view bubbles from transcript messages.

use iocraft::prelude::*;

use elph_tui::TranscriptRowLayout;

use super::super::types::{TranscriptMessage, TranscriptStyle};
use super::kinds::{
    chat_response_card, error_card, meta_card, skill_prompt_card, status_line_card, suppressed_sticky_user_prompt_card,
    thinking_card, thinking_response_pair_card, tool_call_card, user_prompt_card,
};
use super::toggle_ctx::CollapsibleToggleCtx;

/// Extra rows above/below the viewport when windowing transcript cards.
const WINDOW_OVERSCAN_ROWS: u32 = 12;
/// Always keep at least this many trailing messages fully mounted (streaming tail).
const WINDOW_MIN_TAIL_MESSAGES: usize = 6;

/// Subagent click handler type: (agent_id, title).
pub type SubagentClickHandler = HandlerMut<'static, (String, String)>;

pub fn build_transcript_bubbles(
    screen_width: u16,
    messages: &[TranscriptMessage],
    suppress_sticky_source: Option<usize>,
    toggle: Option<CollapsibleToggleCtx>,
) -> Vec<AnyElement<'static>> {
    build_transcript_bubbles_range(screen_width, messages, 0, messages.len(), suppress_sticky_source, toggle, None)
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

    // Ensure we always show at least some content, even if windowing logic fails
    if messages.is_empty() {
        return Vec::new();
    }
    // Defensive: ensure indices are valid and at least one message is shown
    let max_idx = messages.len().saturating_sub(1);
    first_visible = first_visible.min(max_idx);
    last_visible = last_visible.max(first_visible).min(max_idx);
    // Final safety: ensure we always show at least the last message if everything else fails
    if first_visible >= messages.len() || last_visible >= messages.len() || first_visible > last_visible {
        first_visible = max_idx;
        last_visible = max_idx;
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
        None,
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
    _on_subagent_click: Option<&SubagentClickHandler>,
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
    fn live_blank_reply_paints_placeholder_not_blank_box() {
        // A live assistant reply whose content is only whitespace renders a visible ellipsis
        // row — never an empty card (the "blank assistant response" bug).
        let message = TranscriptMessage::assistant_markdown("\n\n   \n");
        let bubbles = build_transcript_bubbles(80, std::slice::from_ref(&message), None, None);
        let rendered = iocraft::prelude::element! { View(width: 80) { #(bubbles) } }.to_string();
        assert!(
            rendered.contains('…'),
            "live blank reply must paint the placeholder, got {rendered:?}"
        );
        assert!(!rendered.trim().is_empty(), "live blank reply must not paint an empty box");

        // Settled (RunCompleted) with empty content → no placeholder, nothing to see.
        let mut settled = TranscriptMessage::assistant_markdown(String::new());
        settled.duration_secs = Some(0.5);
        let bubbles = build_transcript_bubbles(80, std::slice::from_ref(&settled), None, None);
        let rendered = iocraft::prelude::element! { View(width: 80) { #(bubbles) } }.to_string();
        assert!(!rendered.contains('…'), "settled empty reply must not paint a placeholder");
    }

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

    #[test]
    fn windowed_build_always_shows_content_with_large_scroll() {
        let mut messages = Vec::new();
        for i in 0..100 {
            messages.push(TranscriptMessage::text(
                format!("log line {i} with some content to make it multiline and test scrolling behavior"),
                TranscriptStyle::StatusSuccess,
            ));
        }

        let layouts = layout_transcript_rows(&messages, 80);
        let total_rows = layouts
            .last()
            .map(|l| l.start_row.saturating_add(l.row_count))
            .unwrap_or(0);

        // Simulate user scrolling to middle of transcript
        let view_start = total_rows / 2;
        let bubbles = build_transcript_bubbles_windowed(80, &messages, &layouts, view_start, 10, None, None);

        // Should always show at least some bubbles, never empty
        assert!(!bubbles.is_empty(), "windowed build should never return empty bubbles");
    }

    #[test]
    fn windowed_build_with_extreme_scroll_position() {
        let messages = vec![
            TranscriptMessage::text("user prompt", TranscriptStyle::User),
            TranscriptMessage::assistant_markdown("response"),
        ];

        let layouts = layout_transcript_rows(&messages, 80);
        let total_rows = layouts
            .last()
            .map(|l| l.start_row.saturating_add(l.row_count))
            .unwrap_or(0);

        // Test with scroll position beyond content
        let view_start = total_rows.saturating_add(1000);
        let bubbles = build_transcript_bubbles_windowed(80, &messages, &layouts, view_start, 10, None, None);

        // Should still show at least the last message
        assert!(!bubbles.is_empty(), "should show content even with extreme scroll position");
    }

    /// After a long stream completes, scrolling back to the TOP must still render the first
    /// messages in full. This guards against the "scrolled up after stream completes, top
    /// content still clipped" bug: the leading spacer must equal the measured offset of the
    /// first mounted message, and the windowed column must total exactly the measured rows.
    #[test]
    fn windowed_build_scroll_to_top_preserves_leading_content() {
        use iocraft::prelude::*;

        let mut messages = Vec::new();
        for i in 0..60 {
            messages.push(TranscriptMessage::text(
                format!("status log line {i} — a status row with a longer label that stays on one line"),
                TranscriptStyle::StatusSuccess,
            ));
        }
        // A completed assistant reply (stream finished) — stable, cached markdown. We simulate
        // completion by force-flushing the buffer like `mark_stream_complete` → `refresh_stable`.
        let mut assistant = TranscriptMessage::assistant_markdown(
            "## Result\n\nFirst paragraph that wraps across the width nicely.\n\n| Col | Val |\n| --- | --- |\n| a | 1 |\n| b | 2 |\n\nLast paragraph.\n",
        );
        if let Some(md) = assistant.markdown.as_mut() {
            std::sync::Arc::make_mut(md).mark_stream_complete();
            std::sync::Arc::make_mut(md).refresh_stable(&assistant.content, 80);
        }
        messages.push(assistant);

        for width in [50u16, 80, 120] {
            let layouts = layout_transcript_rows(&messages, width);
            let total = layouts
                .last()
                .map(|l| l.start_row.saturating_add(l.row_count))
                .unwrap_or(0);
            let trailing = messages
                .last()
                .map(|m| m.transcript_margin_bottom(None) as u32)
                .unwrap_or(0);

            // Scroll to the TOP — viewport starts at row 0.
            let view_rows = 14u32;
            let bubbles = build_transcript_bubbles_windowed(width, &messages, &layouts, 0, view_rows, None, None);
            let rendered =
                element! { View(width: width, flex_direction: FlexDirection::Column) { #(bubbles) } }.to_string();
            let painted = rendered.lines().count() as u32;
            // Windowed column must still total the full measured height (spacers + mounted).
            assert_eq!(
                total.saturating_add(trailing),
                painted,
                "width {width}: scrolled-to-top windowed paint {painted} != measured total {total}"
            );
            // The very first status message must be present at the top (not skipped).
            assert!(
                rendered.contains("status log line 0"),
                "width {width}: first status message missing when scrolled to top"
            );
        }
    }

    /// Sweep every scroll position (top → bottom) after stream completion. The windowed
    /// column must total the measured height at EVERY offset, and each visible message must
    /// be present in the paint (no gaps or clipped rows).
    #[test]
    fn windowed_build_sweeps_scroll_positions_after_completion() {
        use iocraft::prelude::*;

        let mut messages = Vec::new();
        for i in 0..40 {
            messages.push(TranscriptMessage::text(
                format!("status row {i} with a moderately long single-line label"),
                TranscriptStyle::StatusSuccess,
            ));
        }
        let mut assistant = TranscriptMessage::assistant_markdown(
            "## Result\n\nA completed paragraph.\n\n```mermaid\ngraph TD\n    A[Start] --> B[End]\n```\n\nDone.\n",
        );
        if let Some(md) = assistant.markdown.as_mut() {
            std::sync::Arc::make_mut(md).mark_stream_complete();
            std::sync::Arc::make_mut(md).refresh_stable(&assistant.content, 80);
        }
        messages.push(assistant);

        for width in [60u16, 100] {
            let layouts = layout_transcript_rows(&messages, width);
            let total = layouts
                .last()
                .map(|l| l.start_row.saturating_add(l.row_count))
                .unwrap_or(0);
            let trailing = messages
                .last()
                .map(|m| m.transcript_margin_bottom(None) as u32)
                .unwrap_or(0);
            let expected_total = total.saturating_add(trailing);

            for view_start in [
                0u32,
                total / 4,
                total / 2,
                (total.saturating_mul(3)) / 4,
                total.saturating_sub(1),
            ] {
                let view_rows = 10u32;
                let bubbles =
                    build_transcript_bubbles_windowed(width, &messages, &layouts, view_start, view_rows, None, None);
                let rendered =
                    element! { View(width: width, flex_direction: FlexDirection::Column) { #(bubbles) } }.to_string();
                let painted = rendered.lines().count() as u32;
                assert_eq!(
                    expected_total, painted,
                    "width {width} view_start {view_start}: windowed {painted} != measured {expected_total}"
                );
                // The viewport window must show *something* that belongs to its region:
                // at the top end, the first message is mounted; near the bottom, the tail.
                if view_start == 0 {
                    assert!(
                        rendered.contains("status row 0"),
                        "width {width}: message 0 missing at view_start 0"
                    );
                }
                if view_start >= total.saturating_sub(1) {
                    assert!(
                        rendered.contains("## Result") || rendered.contains("Done."),
                        "width {width}: assistant tail missing near bottom"
                    );
                }
            }
        }
    }
}
