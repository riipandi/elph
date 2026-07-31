//! Scroll-row layout for transcript messages (with incremental cache).

use std::hash::{Hash, Hasher};

use elph_tui::TranscriptRowLayout;
use elph_tui::wrapped_text_row_count;

use super::card::timestamp_layout::{layout_user_input_lines, user_input_right_rail};
use super::markdown::assistant_row_count;
use super::types::{TranscriptMessage, TranscriptStyle};

/// Per-message row-count cache. Invalidated by content fingerprint + wrap width.
#[derive(Debug, Default, Clone)]
pub struct IncrementalLayoutCache {
    screen_width: u16,
    fingerprints: Vec<u64>,
    /// Own bubble height (content + vertical pad), excluding inter-message margin.
    row_counts: Vec<u32>,
    /// Cached start_row for each message so forward walks can resume from the first change.
    start_rows: Vec<u32>,
}

impl IncrementalLayoutCache {
    pub fn clear(&mut self) {
        self.screen_width = 0;
        self.fingerprints.clear();
        self.row_counts.clear();
        self.start_rows.clear();
    }
}

/// Full recompute (tests / cold path without a retained cache).
#[cfg_attr(not(test), allow(dead_code))]
pub fn layout_transcript_rows(messages: &[TranscriptMessage], screen_width: u16) -> Vec<TranscriptRowLayout> {
    let mut cache = IncrementalLayoutCache::default();
    layout_transcript_rows_cached(messages, screen_width, &mut cache)
}

/// Prefer this from the TUI: reuses row counts for unchanged messages (streaming only
/// remeasures the live tail).
pub fn layout_transcript_rows_cached(
    messages: &[TranscriptMessage],
    screen_width: u16,
    cache: &mut IncrementalLayoutCache,
) -> Vec<TranscriptRowLayout> {
    if cache.screen_width != screen_width {
        cache.clear();
        cache.screen_width = screen_width;
    }

    // Truncate / grow slot storage to match the message list.
    if messages.len() < cache.fingerprints.len() {
        cache.fingerprints.truncate(messages.len());
        cache.row_counts.truncate(messages.len());
        cache.start_rows.truncate(messages.len());
    } else if messages.len() > cache.fingerprints.len() {
        cache.fingerprints.resize(messages.len(), 0);
        cache.row_counts.resize(messages.len(), 0);
        cache.start_rows.resize(messages.len(), 0);
    }

    // Walk backward to find the first changed message (streaming usually appends at the tail).
    let mut first_changed = messages.len();
    for index in (0..messages.len()).rev() {
        let message = &messages[index];
        let wrap_width = message.content_inner_width(screen_width);
        let fingerprint = message_layout_fingerprint(message, wrap_width);
        if cache.fingerprints[index] != fingerprint {
            first_changed = index;
            break;
        }
    }

    // Nothing changed — reuse cached start_rows.
    if first_changed == messages.len() {
        return cache
            .start_rows
            .iter()
            .zip(&cache.row_counts)
            .enumerate()
            .map(|(index, (&start_row, &row_count))| TranscriptRowLayout { start_row, row_count })
            .collect();
    }

    // Recompute from first_changed - 1 because margin_bottom of the previous message
    // depends on this message's style.
    let start = first_changed.saturating_sub(1);
    let mut cursor = if start == 0 {
        0u32
    } else if cache.start_rows[start - 1] == 0 && cache.row_counts[start - 1] == 0 {
        // Prefix cache is stale (e.g. fresh cache or history rewrite) — walk from the beginning.
        let mut c = 0u32;
        for i in 0..start {
            let row_count = cache.row_counts[i];
            c = c.saturating_add(row_count);
            if i + 1 < messages.len() {
                c = c.saturating_add(messages[i].transcript_margin_bottom(messages.get(i + 1)) as u32);
            }
        }
        c
    } else {
        cache.start_rows[start - 1]
            .saturating_add(cache.row_counts[start - 1])
            .saturating_add(messages[start - 1].transcript_margin_bottom(Some(&messages[start])) as u32)
    };

    let mut layouts = Vec::with_capacity(messages.len());

    // Emit cached prefix (messages before `start`).
    for index in 0..start {
        layouts.push(TranscriptRowLayout {
            start_row: cache.start_rows[index],
            row_count: cache.row_counts[index],
        });
    }

    // Emit recomputed suffix from `start`.
    for (index, message) in messages.iter().enumerate().skip(start) {
        let wrap_width = message.content_inner_width(screen_width);
        let fingerprint = message_layout_fingerprint(message, wrap_width);
        if cache.fingerprints[index] != fingerprint {
            cache.fingerprints[index] = fingerprint;
            cache.row_counts[index] = message_row_count(message, wrap_width);
        }
        let row_count = cache.row_counts[index];
        cache.start_rows[index] = cursor;
        layouts.push(TranscriptRowLayout {
            start_row: cursor,
            row_count,
        });
        cursor = cursor.saturating_add(row_count);
        if index + 1 < messages.len() {
            cursor = cursor.saturating_add(message.transcript_margin_bottom(messages.get(index + 1)) as u32);
        }
    }

    layouts
}

fn message_row_count(message: &TranscriptMessage, wrap_width: u16) -> u32 {
    let row_count = if message.style == TranscriptStyle::Assistant {
        if message.is_response_collapsed() {
            1
        } else {
            let body = assistant_row_count(&message.content, message.markdown.as_ref(), wrap_width) as u32;
            // Slash responses paint without the phase header (see `chat_response_card`).
            let header_and_gap = if message.local_slash_response {
                0
            } else if body > 0 {
                2
            } else {
                1
            };
            body.saturating_add(header_and_gap)
        }
    } else if message.style.is_user_input_card() {
        let right_rail = user_input_right_rail(message.submitted_at, message.duration_secs);
        layout_user_input_lines(&message.content, right_rail.as_deref(), wrap_width).len() as u32
    } else {
        // Plain-text cards (thinking body, tool output, status) paint with word-wrap; measure
        // with the same wrap so the scroll viewport matches the painted height exactly.
        let text = message.layout_text();
        let mut rows = wrapped_text_row_count(&text, wrap_width as usize).min(u32::MAX as usize) as u32;
        // thinking_card paints a 1-row gap between the phase header and the body
        // (phase_card_shell gap), which layout_text's "header\nbody" has no blank row for.
        if message.style == TranscriptStyle::Thinking && text.contains('\n') {
            rows = rows.saturating_add(1);
        }
        rows
    };
    let vertical_pad = message
        .transcript_padding_top()
        .saturating_add(message.transcript_padding_bottom()) as u32;
    row_count.saturating_add(vertical_pad)
}

/// Cheap content fingerprint — samples ends so streaming appends invalidate without hashing full body.
fn message_layout_fingerprint(message: &TranscriptMessage, wrap_width: u16) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    wrap_width.hash(&mut hasher);
    std::mem::discriminant(&message.style).hash(&mut hasher);
    message.detail_expanded.hash(&mut hasher);
    message.local_slash_response.hash(&mut hasher);
    message.status_indent.hash(&mut hasher);
    if let Some(secs) = message.duration_secs {
        secs.to_bits().hash(&mut hasher);
    } else {
        0u64.hash(&mut hasher);
    }
    hash_text_sample(&message.content, &mut hasher);
    if let Some(detail) = message.status_detail.as_deref() {
        hash_text_sample(detail, &mut hasher);
    }
    if let Some(tool) = &message.tool {
        tool.name.hash(&mut hasher);
        hash_text_sample(&tool.args_summary, &mut hasher);
        tool.output.len().hash(&mut hasher);
        hash_text_sample(&tool.output, &mut hasher);
        if let Some(ref old) = tool.old_text {
            old.len().hash(&mut hasher);
            hash_text_sample(old, &mut hasher);
        }
        if let Some(ref new) = tool.new_text {
            new.len().hash(&mut hasher);
            hash_text_sample(new, &mut hasher);
        }
    }
    if let Some(md) = &message.markdown {
        md.stable_end.hash(&mut hasher);
        md.stream_complete.hash(&mut hasher);
        md.wrap_width.hash(&mut hasher);
        if let Some(part) = md.parts.first() {
            part.source_hash.hash(&mut hasher);
            part.row_count.hash(&mut hasher);
            part.document.is_some().hash(&mut hasher);
        }
    }
    if let Some(at) = message.submitted_at {
        at.timestamp().hash(&mut hasher);
    }
    hasher.finish()
}

fn hash_text_sample(text: &str, hasher: &mut impl Hasher) {
    text.len().hash(hasher);
    if text.len() <= 96 {
        text.hash(hasher);
        return;
    }
    // Char-safe samples — never byte-slice mid UTF-8 (multi-byte tool/stream text panics).
    for c in text.chars().take(24) {
        c.hash(hasher);
    }
    for c in text.chars().rev().take(24) {
        c.hash(hasher);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::transcript::card::FLUSH_CARD_PAD;
    use crate::tui::transcript::types::{EPHEMERAL_NOTICE_EXTRA_PAD_TOP, TranscriptMessage, TranscriptStyle};

    #[test]
    fn ephemeral_notice_row_layout_includes_extra_top_padding() {
        let messages = vec![
            TranscriptMessage::assistant_markdown("reply"),
            TranscriptMessage::startup_status("transient:agent_mode", "Agent mode: plan.", TranscriptStyle::Meta),
        ];
        let layouts = layout_transcript_rows(&messages, 80);
        let notice = &layouts[1];
        let reply = &layouts[0];
        let notice_pad = (FLUSH_CARD_PAD + EPHEMERAL_NOTICE_EXTRA_PAD_TOP) as u32 * 2;
        assert_eq!(notice.start_row, reply.start_row.saturating_add(reply.row_count));
        assert!(notice.row_count >= notice_pad);
    }

    #[test]
    fn incremental_cache_reuses_stable_prefix_row_counts() {
        let mut messages = vec![
            TranscriptMessage::text("user hi", TranscriptStyle::User),
            TranscriptMessage::text("a", TranscriptStyle::Assistant),
        ];
        let mut cache = IncrementalLayoutCache::default();
        let first = layout_transcript_rows_cached(&messages, 80, &mut cache);
        let fp_user = cache.fingerprints[0];
        let rows_user = cache.row_counts[0];

        // Stream more assistant text — user slot must stay cached.
        if let Some(last) = messages.last_mut() {
            last.content.push_str(" more tokens from the model");
        }
        let second = layout_transcript_rows_cached(&messages, 80, &mut cache);
        assert_eq!(cache.fingerprints[0], fp_user);
        assert_eq!(cache.row_counts[0], rows_user);
        assert_eq!(second[0].row_count, first[0].row_count);
        // Assistant grew → more rows (or at least not fewer).
        assert!(second[1].row_count >= first[1].row_count);
    }

    #[test]
    fn width_change_invalidates_cache() {
        let messages = vec![TranscriptMessage::text(
            "hello world from cache",
            TranscriptStyle::Assistant,
        )];
        let mut cache = IncrementalLayoutCache::default();
        let wide = layout_transcript_rows_cached(&messages, 120, &mut cache);
        let narrow = layout_transcript_rows_cached(&messages, 20, &mut cache);
        // Narrow wrap should not panic and usually needs more rows.
        assert!(!wide.is_empty() && !narrow.is_empty());
    }

    #[test]
    fn layout_fingerprint_handles_multibyte_utf8_without_panic() {
        // Multi-byte stream content previously panicked on mid-codepoint byte slices.
        let mut long = "✓ ".repeat(80);
        long.push_str(&"名".repeat(40));
        long.push_str("\ntrail");
        let messages = vec![TranscriptMessage::text(long, TranscriptStyle::Thinking)];
        let mut cache = IncrementalLayoutCache::default();
        let layouts = layout_transcript_rows_cached(&messages, 80, &mut cache);
        assert_eq!(layouts.len(), 1);
        assert!(layouts[0].row_count >= 1);
    }

    #[test]
    fn slash_response_measure_matches_paint_without_header() {
        use iocraft::prelude::*;
        let content = "## Tools\n\n- `find_path` — Quickly finds files by matching glob patterns (like `*.rs`), returning matching file paths alphabetically.\n- `list_dir` — Lists files and directories.\n\n| Tool | Group |\n| --- | --- |\n| `read_file` | Read |\n| `write_file` | Write |\n";
        let message = TranscriptMessage::assistant_slash_markdown(content);
        for width in [36u16, 40, 80, 120] {
            let layouts = layout_transcript_rows(&[message.clone()], width);
            let row_count = layouts[0].row_count;
            let margin = message.transcript_margin_bottom(None) as u32;
            let bubbles = crate::tui::transcript::card::build_transcript_bubbles(width, &[message.clone()], None, None);
            let rendered = element! { View(width: width) { #(bubbles) } }.to_string();
            let painted = rendered.lines().count() as u32;
            assert_eq!(
                row_count.saturating_add(margin),
                painted,
                "width {width}: measured {} (+ margin {margin}) != painted {painted}",
                row_count,
            );
            // Slash responses are complete local output — no "Response" phase header.
            assert!(
                !rendered.contains("Response"),
                "slash card must not render a Response header at width {width}"
            );
        }
    }

    #[test]
    fn slash_flow_full_and_windowed_heights_match_measure() {
        use iocraft::prelude::*;
        let content = "## Tools\n\n- `find_path` — Quickly finds files by matching glob patterns (like `*.rs`), returning matching file paths alphabetically.\n- `list_dir` — Lists files and directories.\n\n| Tool | Group |\n| --- | --- |\n| `read_file` | Read |\n| `write_file` | Write |\n";
        let messages = vec![
            TranscriptMessage::text("/tools list", TranscriptStyle::User),
            TranscriptMessage::assistant_slash_markdown(content),
        ];
        for width in [36u16, 40, 60, 80, 120] {
            let layouts = layout_transcript_rows(&messages, width);
            let total = layouts
                .last()
                .map(|l| l.start_row.saturating_add(l.row_count))
                .unwrap_or(0);
            // Painted full tree includes the last message's trailing margin.
            let trailing_margin = messages
                .last()
                .map(|m| m.transcript_margin_bottom(None) as u32)
                .unwrap_or(0);
            let expected = total.saturating_add(trailing_margin);

            let full = crate::tui::transcript::card::build_transcript_bubbles(width, &messages, None, None);
            let full_text =
                element! { View(width: width, flex_direction: FlexDirection::Column) { #(full) } }.to_string();
            let full_rows = full_text.lines().count() as u32;
            assert_eq!(
                expected, full_rows,
                "width {width}: full paint {full_rows} != measured total {total} + trailing margin {trailing_margin}"
            );

            let view_rows = 20u32;
            let view_start = total.saturating_sub(view_rows);
            let windowed = crate::tui::transcript::card::build_transcript_bubbles_windowed(
                width, &messages, &layouts, view_start, view_rows, None, None,
            );
            let windowed_rows = element! { View(width: width, flex_direction: FlexDirection::Column) { #(windowed) } }
                .to_string()
                .lines()
                .count() as u32;
            assert_eq!(
                expected, windowed_rows,
                "width {width}: windowed paint {windowed_rows} != expected {expected}"
            );
        }
    }

    /// Measure–paint parity for every card kind, including the Thinking+Assistant flush pair.
    ///
    /// The auto-scroll viewport pins its bottom to the *measured* total. Any divergence
    /// (extra painted gap rows, missing vertical pad, pair counted as two padded cards)
    /// shifts the window so cards appear clipped mid-line — the `/tools list` fragment bug.
    #[test]
    fn all_card_kinds_measure_matches_paint() {
        use iocraft::prelude::*;

        let thinking_text = "I should verify how the transcript measures rows against what iocraft \
            paints. The wrap width has to match the card padding exactly, and the flush pair must \
            not double-count vertical padding, otherwise the viewport drifts and mid-card lines get \
            clipped at the bottom of the window."
            .to_string();
        let mut thinking_stream = TranscriptMessage::text(&thinking_text, TranscriptStyle::Thinking);
        let mut thinking_done = TranscriptMessage::text(&thinking_text, TranscriptStyle::Thinking);
        thinking_done.duration_secs = Some(1.5);
        let mut thinking_done_collapsed = thinking_done.clone();
        thinking_done_collapsed.detail_expanded = false;

        let mut assistant_stream = TranscriptMessage::assistant_markdown(
            "Let me lay out the steps.\n\nFirst I will measure the painted height, then compare it \
            against the measured row count at every terminal width so the auto-scroll viewport stays \
            pinned exactly to the bottom of the transcript.",
        );
        let mut assistant_done = TranscriptMessage::assistant_markdown(
            "Here is the plan:\n\n- Measure the painted height with iocraft at the card's real inner \
            width.\n- Compare it against the layout row count including margins.\n- Fix any drift so \
            the viewport never clips a card mid-line.",
        );
        assistant_done.duration_secs = Some(2.0);
        let mut assistant_done_collapsed = assistant_done.clone();
        assistant_done_collapsed.detail_expanded = false;

        let user_prompt = TranscriptMessage::text("what does this module do?", TranscriptStyle::User);
        let status = TranscriptMessage::text("read_file src/main.rs", TranscriptStyle::StatusSuccess);

        let mut tool_running =
            TranscriptMessage::tool_call("read_file", r#"{"path":"src/main.rs"}"#, TranscriptStyle::ToolRunning);
        let mut tool_done_expanded =
            TranscriptMessage::tool_call("read_file", r#"{"path":"src/main.rs"}"#, TranscriptStyle::ToolSuccess);
        tool_done_expanded.duration_secs = Some(0.4);
        tool_done_expanded.tool.as_mut().unwrap().output = "fn main() {\n    println!(\"hello\");\n}\n".to_string();
        let mut tool_done_collapsed = tool_done_expanded.clone();
        tool_done_collapsed.detail_expanded = false;

        let mut ask_user = TranscriptMessage::tool_call(
            "ask_user_question",
            r#"{"question":"Continue?","options":["Yes","No"]}"#,
            TranscriptStyle::ToolSuccess,
        );
        ask_user.duration_secs = Some(0.2);
        ask_user.tool.as_mut().unwrap().output = "User answered: Yes".to_string();

        let mut tool_diff =
            TranscriptMessage::tool_call("edit_file", r#"{"path":"src/main.rs"}"#, TranscriptStyle::ToolSuccess);
        tool_diff.duration_secs = Some(0.6);
        {
            let detail = tool_diff.tool.as_mut().unwrap();
            detail.old_text = Some("fn old() {}\n".to_string());
            detail.new_text = Some("fn new() {}\n".to_string());
            detail.file_path = Some("src/main.rs".to_string());
        }

        let cases: Vec<(&str, Vec<TranscriptMessage>)> = vec![
            ("thinking streaming", vec![thinking_stream]),
            ("thinking done expanded", vec![thinking_done.clone()]),
            ("thinking done collapsed", vec![thinking_done_collapsed.clone()]),
            ("assistant streaming", vec![assistant_stream]),
            ("assistant done expanded", vec![assistant_done.clone()]),
            ("assistant done collapsed", vec![assistant_done_collapsed.clone()]),
            ("user prompt", vec![user_prompt]),
            ("status row", vec![status]),
            ("tool running expanded", vec![tool_running]),
            ("tool done expanded", vec![tool_done_expanded.clone()]),
            ("tool done collapsed", vec![tool_done_collapsed]),
            ("ask user tool", vec![ask_user]),
            ("tool with diff", vec![tool_diff]),
            (
                "pair thinking+assistant collapsed",
                vec![thinking_done_collapsed, assistant_done_collapsed],
            ),
            ("pair thinking+assistant expanded", vec![thinking_done, assistant_done]),
        ];

        for (name, messages) in &cases {
            for width in [36u16, 60, 120] {
                let layouts = layout_transcript_rows(messages, width);
                let mut expected = 0u32;
                for (index, layout) in layouts.iter().enumerate() {
                    expected += layout.row_count;
                    expected += messages[index].transcript_margin_bottom(messages.get(index + 1)) as u32;
                }
                let bubbles = crate::tui::transcript::card::build_transcript_bubbles(width, messages, None, None);
                let rendered =
                    element! { View(width: width, flex_direction: FlexDirection::Column) { #(bubbles) } }.to_string();
                let painted = rendered.lines().count() as u32;
                assert_eq!(
                    expected, painted,
                    "{name} width {width}: measured {expected} != painted {painted}"
                );
            }
        }
    }
}
