//! Scroll-row layout for transcript messages.

use elph_tui::{TranscriptRowLayout, transcript_bubble_inner_width, wrapped_transcript_row_count};

use super::types::TranscriptMessage;

pub fn layout_transcript_rows(messages: &[TranscriptMessage], screen_width: u16) -> Vec<TranscriptRowLayout> {
    let mut layouts = Vec::with_capacity(messages.len());
    let mut cursor = 0u32;
    for (index, message) in messages.iter().enumerate() {
        let wrap_width = transcript_bubble_inner_width(screen_width, message.style.horizontal_padding());
        let row_count = wrapped_transcript_row_count(&message.layout_text(), wrap_width) as u32;
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
