//! Scroll-row layout for transcript messages (with incremental cache).

use std::hash::{Hash, Hasher};

use elph_tui::TranscriptRowLayout;
use elph_tui::{transcript_bubble_inner_width, wrapped_transcript_row_count};

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
}

impl IncrementalLayoutCache {
    pub fn clear(&mut self) {
        self.screen_width = 0;
        self.fingerprints.clear();
        self.row_counts.clear();
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
        // History was rewritten shorter — drop stale slots.
        cache.fingerprints.truncate(messages.len());
        cache.row_counts.truncate(messages.len());
    } else if messages.len() > cache.fingerprints.len() {
        cache.fingerprints.resize(messages.len(), 0);
        cache.row_counts.resize(messages.len(), 0);
    }

    for (index, message) in messages.iter().enumerate() {
        let wrap_width = transcript_bubble_inner_width(screen_width, message.style.horizontal_padding())
            .saturating_sub(message.style.content_chrome_cols())
            .max(1);
        let fingerprint = message_layout_fingerprint(message, wrap_width);
        if cache.fingerprints[index] != fingerprint {
            cache.fingerprints[index] = fingerprint;
            cache.row_counts[index] = message_row_count(message, wrap_width);
        }
    }

    // start_row walk is O(n) but cheap (no wrap); margins depend on neighbors.
    let mut layouts = Vec::with_capacity(messages.len());
    let mut cursor = 0u32;
    for (index, message) in messages.iter().enumerate() {
        let row_count = cache.row_counts[index];
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
            let header_and_gap = if body > 0 { 2 } else { 1 };
            body.saturating_add(header_and_gap)
        }
    } else if message.style.is_user_input_card() {
        let right_rail = user_input_right_rail(message.submitted_at, message.duration_secs);
        layout_user_input_lines(&message.content, right_rail.as_deref(), wrap_width).len() as u32
    } else {
        wrapped_transcript_row_count(&message.layout_text(), wrap_width) as u32
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
}
