//! Scroll-row layout for transcript messages.

use elph_tui::TranscriptRowLayout;
use elph_tui::{transcript_bubble_inner_width, wrapped_transcript_row_count};

use super::card::timestamp_layout::{layout_user_input_lines, user_input_right_rail};
use super::markdown::assistant_row_count;
use super::types::{TranscriptMessage, TranscriptStyle};

pub fn layout_transcript_rows(messages: &[TranscriptMessage], screen_width: u16) -> Vec<TranscriptRowLayout> {
    let mut layouts = Vec::with_capacity(messages.len());
    let mut cursor = 0u32;
    for (index, message) in messages.iter().enumerate() {
        let wrap_width = transcript_bubble_inner_width(screen_width, message.style.horizontal_padding())
            .saturating_sub(message.style.content_chrome_cols())
            .max(1);
        let row_count = if message.style == TranscriptStyle::Assistant {
            assistant_row_count(&message.content, message.markdown.as_ref(), wrap_width) as u32
        } else if message.style.is_user_input_card() {
            let right_rail = user_input_right_rail(message.submitted_at, message.duration_secs);
            layout_user_input_lines(&message.content, right_rail.as_deref(), wrap_width).len() as u32
        } else {
            wrapped_transcript_row_count(&message.layout_text(), wrap_width) as u32
        };
        layouts.push(TranscriptRowLayout {
            start_row: cursor,
            row_count,
        });
        cursor = cursor.saturating_add(row_count);
        if index + 1 < messages.len() {
            let next_style = messages.get(index + 1).map(|m| m.style);
            cursor = cursor.saturating_add(message.style.entry_gap_after(next_style) as u32);
        }
    }
    layouts
}
