use elph_tui::text_input_layout::*;

#[test]
fn row_count_matches_newlines() {
    let layout = WrappedTextLayout::new("a\nb\nc", 20);
    assert_eq!(layout.row_count(), 3);
}

#[test]
fn row_column_on_second_line() {
    let text = "a\nb";
    let layout = WrappedTextLayout::new(text, 20);
    assert_eq!(layout.row_column_for_offset(2), (1, 0));
}

#[test]
fn wrap_width_reserves_cursor_column() {
    assert_eq!(text_input_wrap_width(10), 9);
    assert_eq!(text_input_wrap_width(0), 0);
}

#[test]
fn empty_text_has_single_row() {
    let layout = WrappedTextLayout::new("", 20);
    assert_eq!(layout.row_count(), 1);
    assert_eq!(layout.row_column_for_offset(0), (0, 0));
}

#[test]
fn trailing_newline_row_at_eof() {
    let text = "asd\n";
    let layout = WrappedTextLayout::new(text, 10);
    assert_eq!(layout.row_count(), 2);
    assert_eq!(layout.row_column_for_offset(text.len()), (1, 0));
}

#[test]
fn soft_wrap_splits_long_line() {
    let layout = WrappedTextLayout::new("1234567890", 6);
    assert_eq!(layout.row_count(), 2);
    assert_eq!(layout.row_column_for_offset(5), (0, 5));
    assert_eq!(layout.row_column_for_offset(6), (1, 1));
}

#[test]
fn empty_continuation_line_after_newline() {
    let text = "hello\n";
    let layout = WrappedTextLayout::new(text, 10);
    assert_eq!(layout.row_column_for_offset("hello".len()), (0, 5));
}

#[test]
fn update_scroll_offset_zero_viewport() {
    assert_eq!(update_scroll_offset(3, 5, 0, 10), 0);
}

#[test]
fn update_scroll_offset_clamps_to_max() {
    assert_eq!(update_scroll_offset(0, 9, 3, 5), 2);
}

#[test]
fn update_scroll_offset_keeps_cursor_visible_when_scrolling_up() {
    assert_eq!(update_scroll_offset(5, 1, 3, 10), 1);
}
