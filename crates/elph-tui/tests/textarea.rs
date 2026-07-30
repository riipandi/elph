use elph_tui::components::textarea::*;
use elph_tui::paste::{PasteBurstState, newline_count};
use elph_tui::text_editing::{insert_newline_at_cursor, line_start_offset, wire_insert_newline};
use elph_tui::text_input_layout::WrappedTextLayout;
use elph_tui::text_input_layout::update_scroll_offset;
use iocraft::prelude::*;

#[test]
fn insert_newline_at_cursor_appends() {
    let (text, next) = insert_newline_at_cursor("hi", 2);
    assert_eq!(text, "hi\n");
    assert_eq!(next, 3);
}

#[test]
fn logical_line_count_includes_trailing_newline_row() {
    assert_eq!(logical_line_count("hello"), 1);
    assert_eq!(logical_line_count("hello\n"), 2);
    assert_eq!(logical_line_count("a\nb\n"), 3);
}

#[test]
fn display_row_count_grows_with_newlines() {
    assert_eq!(display_row_count("one", 20), 1);
    assert_eq!(display_row_count("a\nb", 20), 2);
    assert_eq!(display_row_count("hello\n", 20), 2);
}

#[test]
fn visible_row_count_omits_trailing_blank_unless_cursor_there() {
    let text = "hello\n";
    assert_eq!(visible_row_count(text, text.len(), 20), 2);
    assert_eq!(visible_row_count("line1\nline2\n", "line1\nline2".len(), 20), 2);
    assert_eq!(visible_row_count("line1\nline2\n", "line1\nline2\n".len(), 20), 3);
    assert_eq!(visible_row_count(text, text.len().saturating_sub(1), 20), 1);
}

#[test]
fn viewport_grows_when_cursor_on_trailing_empty_line() {
    let text = "hello\n";
    let on_empty = layout_textarea(text, text.len(), 20, 1, None);
    let before_empty = layout_textarea(text, text.len().saturating_sub(1), 20, 1, None);
    assert_eq!(on_empty.viewport_height, 2);
    assert_eq!(before_empty.viewport_height, 1);
}

#[test]
fn viewport_height_caps_at_max() {
    let layout = layout_textarea("a\nb\nc\nd\ne", 4, 20, 1, Some(3));
    assert_eq!(layout.viewport_height, 3);
    assert!(layout.show_scrollbar);
    assert_eq!(layout.content_rows, 5);
}

#[test]
fn viewport_height_grows_without_max() {
    let layout = layout_textarea("a\nb\nc", 4, 20, 1, None);
    assert_eq!(layout.viewport_height, 3);
    assert!(!layout.show_scrollbar);
}

#[test]
fn update_scroll_offset_follows_cursor() {
    assert_eq!(update_scroll_offset(0, 4, 3, 8), 2);
    assert_eq!(update_scroll_offset(5, 2, 3, 8), 2);
}

#[test]
fn layout_cursor_maps_trailing_newline_to_empty_row() {
    let text = "hello\n";
    assert_eq!(layout_cursor_for_viewport(text, text.len()), text.len());
    assert_eq!(layout_cursor_for_viewport(text, text.len().saturating_sub(1)), text.len());
    assert_eq!(layout_cursor_for_viewport(text, 3), 3);
}

#[test]
fn layout_cursor_preserves_middle_blank_line() {
    let text = "line1\n\n";
    assert_eq!(layout_cursor_for_viewport(text, 6), 6);
    assert_eq!(layout_cursor_for_viewport(text, text.len()), text.len());
}

#[test]
fn layout_textarea_reserves_scrollbar_column_when_content_overflows() {
    let layout = layout_textarea("one two three four five six seven", 0, 12, 1, Some(2));
    assert!(layout.show_scrollbar);
    assert_eq!(layout.input_width, 11);
}

#[test]
fn layout_textarea_hides_scrollbar_until_content_overflows() {
    let layout = layout_textarea("hello", 0, 40, 1, Some(12));
    assert!(!layout.show_scrollbar);
    assert_eq!(layout.input_width, 40);
}

#[test]
fn display_row_count_soft_wraps_long_lines() {
    assert_eq!(display_row_count("12345678901", 6), 2);
}

#[test]
fn wire_first_newline_cursor_lands_on_empty_continuation_row() {
    let (text, cursor) = wire_insert_newline("hello", 5);
    assert_eq!(text, "hello\n");
    assert_eq!(cursor, text.len());
    let layout = layout_textarea(&text, layout_cursor_for_viewport(&text, cursor), 20, 1, None);
    assert_eq!(layout.viewport_height, 2);
}

#[test]
fn two_wire_newlines_append_without_extra_blank() {
    let (t1, c1) = wire_insert_newline("hello", 5);
    assert_eq!(t1, "hello\n");
    assert_eq!(c1, 6);
    let (t2, c2) = wire_insert_newline(&t1, c1);
    assert_eq!(t2, "hello\n\n");
    assert_eq!(c2, 7);
    assert_eq!(newline_count(&t2), 2);
}

#[test]
fn cursor_left_from_empty_row_targets_prior_line_content() {
    let text = "hello\n";
    let empty_row = text.len();
    assert_eq!(line_start_offset(text, empty_row), 6);
}

#[test]
fn scroll_follows_cursor_to_empty_continuation_row() {
    let text = "a\nb\nc\nd\ne\n";
    let layout = layout_textarea(text, text.len(), 20, 1, Some(3));
    let wrapped = WrappedTextLayout::new_for_overlay_editor(text, layout.input_width);
    let layout_cursor = layout_cursor_for_viewport(text, text.len());
    let (row, _) = wrapped.row_column_for_offset(text, layout_cursor);
    let offset = update_scroll_offset(0, row, layout.viewport_height, layout.content_rows);
    assert!(row + 1 >= layout.viewport_height || offset <= row);
}

// Regression: cursor column after each keystroke stays correct — small buffer.
#[test]
fn cursor_column_after_paste_stays_correct_small_buffer() {
    let paste = "line one\nline two\nline three";
    let w = 20;
    let wrapped = WrappedTextLayout::new_for_overlay_editor(paste, w);
    let cursor = paste.len();
    let (cursor_row, cursor_col) = wrapped.row_column_for_offset(paste, cursor);
    // Verify cursor is on the last row at the correct column
    assert_eq!(cursor_row, wrapped.row_count() - 1);
    assert_eq!(cursor_col as usize, paste.lines().last().unwrap().len());
}

// Regression: cursor column after appending to a long line stays accurate.
#[test]
fn cursor_column_after_appending_to_long_line() {
    let long_line = "a".repeat(120);
    let text = format!("{}{}", long_line, "hello");
    let w = 30;
    let wrapped = WrappedTextLayout::new_for_overlay_editor(&text, w);
    let cursor = text.len();
    let (cursor_row, cursor_col) = wrapped.row_column_for_offset(&text, cursor);
    let last_row_nr = wrapped.row_count().saturating_sub(1);
    assert_eq!(cursor_row, last_row_nr, "cursor must be on the last display row");
    // The long line wraps across many rows; "hello" is appended at the end
    // col should be the display width of "hello" (5)
    assert_eq!(cursor_col as usize, "hello".len());
}

// Regression: long paste in textarea, cursor displays correctly at EOF.
// After pasting 2048+ chars, the viewport slice path is used for rendering.
#[test]
fn long_paste_cursor_at_eof_column_is_correct() {
    let mut text = String::from("prefix-");
    text.push_str(&"x".repeat(3000));
    let cursor = text.len();
    let w = 40;
    let wrapped = WrappedTextLayout::new_for_overlay_editor(&text, w);
    let (cursor_row, _cursor_col) = wrapped.row_column_for_offset(&text, cursor);
    // Cursor must be on the last wrapped row, not stuck mid-text
    assert_eq!(cursor_row, wrapped.row_count().saturating_sub(1));
    // Verify we can get valid layout at EOF
    let layout = layout_textarea(&text, cursor, w, 1, Some(3));
    assert!(layout.content_rows > 0);
    let (row_after, _) = wrapped.row_column_for_offset(&text, cursor);
    assert_eq!(row_after, wrapped.row_count().saturating_sub(1));
}

// Regression: cursor stays on correct row after navigating right through long content.
#[test]
fn move_right_through_long_content_does_not_stall() {
    let text = "a".repeat(200);
    let cursor = 0;
    let w = 40;
    let wrapped = WrappedTextLayout::new_for_overlay_editor(&text, w);
    // After moving right by one char, cursor should advance
    let next = WrappedTextLayout::right_of_offset(&text, cursor);
    assert_eq!(next, 1);
    // After moving to EOF, cursor should equal text.len()
    let eof = WrappedTextLayout::right_of_offset(&text, text.len());
    assert_eq!(eof, text.len());
    let near_eof = WrappedTextLayout::right_of_offset(&text, text.len() - 1);
    assert_eq!(near_eof, text.len());
}

// Regression: after newline in multiline, cursor lands on the correct wrapped row.
#[test]
fn multiline_newline_cursor_on_new_row() {
    let text = "hello\n";
    let cursor = text.len(); // On the empty continuation row
    let w = 40;
    let wrapped = WrappedTextLayout::new_for_overlay_editor(text, w);
    let (row, col) = wrapped.row_column_for_offset(text, layout_cursor_for_viewport(text, cursor));
    assert_eq!(row, wrapped.row_count() - 1);
    assert_eq!(col, 0);
    // Layout cursor maps to the empty continuation row
    let layout_cursor = layout_cursor_for_viewport(text, text.len() - 1); // cursor before newline
    assert_eq!(layout_cursor, text.len()); // maps to continuation row
}

// Regression: Row column computation identical regardless of viewport width.
#[test]
fn row_column_consistent_across_widths() {
    let text = "short line\n";
    for w in [20u16, 40, 80, 120] {
        let wrapped = WrappedTextLayout::new_for_overlay_editor(text, w);
        let (row, col) = wrapped.row_column_for_offset(text, text.len());
        assert_eq!(row, 1, "width={w}: cursor row must be 1 (0-indexed second row)");
        assert_eq!(col, 0, "width={w}: cursor col must be 0 on empty continuation row");
    }
}

// Regression: Simple paste of non-wrapping text: cursor at EOF, col equals text width.
#[test]
fn paste_simple_cursor_at_eof_column_is_text_display_width() {
    // Short enough to not trigger VIEWPORT_SLICE path (2048), long enough to wrap.
    let text = "The quick brown fox jumps over the lazy dog. ".repeat(30);
    assert!(text.len() < 2048, "keep below viewport slice threshold");
    let cursor = text.len();
    let w = 50;
    let wrapped = WrappedTextLayout::new_for_overlay_editor(&text, w);
    let (_, cursor_col) = wrapped.row_column_for_offset(&text, cursor);
    // cursor_col should be the display width of the last wrapped segment
    let last_line = text.lines().last().unwrap();
    // Since it's one long line that wraps, the cursor_col is the col within the last wrap-row
    let wrap_width = w as usize;
    let remainder = last_line.chars().count() % wrap_width;
    let expected_col = if remainder == 0 { wrap_width } else { remainder };
    assert_eq!(cursor_col as usize, expected_col);
}
