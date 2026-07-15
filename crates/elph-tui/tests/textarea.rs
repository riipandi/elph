use elph_tui::components::textarea::*;
use elph_tui::paste::newline_count;
use elph_tui::text_editing::{insert_newline_at_cursor, line_start_offset, wire_insert_newline};
use elph_tui::text_input_layout::{WrappedTextLayout, update_scroll_offset};

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
fn layout_textarea_reserves_scrollbar_column() {
    let layout = layout_textarea("one two three four five six seven", 0, 12, 1, Some(2));
    assert!(layout.show_scrollbar);
    assert_eq!(layout.input_width, 11);
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
