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
    assert_eq!(layout.row_column_for_offset(text, 2), (1, 0));
}

#[test]
fn wrap_width_reserves_cursor_column() {
    assert_eq!(text_input_wrap_width(10), 9);
    assert_eq!(text_input_wrap_width(0), 0);
    assert_eq!(overlay_editor_wrap_width(10), 10);
}

#[test]
fn overlay_editor_eof_cursor_on_last_wrapped_row() {
    let text = "**Elph** is a Rust workspace for AI agent applications: a coding agent CLI, shared agent runtime libraries, and terminal UI components. It is a port of the [pi](https://pi.dev) TypeScript ecosystem to Rust, with additional MCP (Model Context Protocol) support, WASM extensions, and an iocraft-based interactive TUI.";
    let layout = WrappedTextLayout::new_for_overlay_editor(text, 72);
    let (row, _) = layout.row_column_for_offset(text, text.len());
    assert_eq!(row, layout.row_count().saturating_sub(1));
}

#[test]
fn empty_text_has_single_row() {
    let layout = WrappedTextLayout::new("", 20);
    assert_eq!(layout.row_count(), 1);
    assert_eq!(layout.row_column_for_offset("", 0), (0, 0));
}

#[test]
fn trailing_newline_row_at_eof() {
    let text = "asd\n";
    let layout = WrappedTextLayout::new(text, 10);
    assert_eq!(layout.row_count(), 2);
    assert_eq!(layout.row_column_for_offset(text, text.len()), (1, 0));
}

#[test]
fn soft_wrap_splits_long_line() {
    let layout = WrappedTextLayout::new("1234567890", 6);
    assert_eq!(layout.row_count(), 2);
    assert_eq!(layout.row_column_for_offset("1234567890", 4), (0, 4));
    assert_eq!(layout.row_column_for_offset("1234567890", 5), (1, 0));
    assert_eq!(layout.row_column_for_offset("1234567890", 6), (1, 1));
}

#[test]
fn empty_continuation_line_after_newline() {
    let text = "hello\n";
    let layout = WrappedTextLayout::new(text, 10);
    assert_eq!(layout.row_column_for_offset(text, "hello".len()), (0, 5));
}

#[test]
fn update_scroll_offset_zero_viewport() {
    assert_eq!(update_scroll_offset(3, 5, 0, 10), 0);
}

#[test]
fn wrap_empty_line_segment() {
    let layout = WrappedTextLayout::new("a\n\nb", 10);
    assert!(layout.row_count() >= 3);
}

#[test]
fn update_scroll_offset_clamps_to_max() {
    assert_eq!(update_scroll_offset(0, 9, 3, 5), 2);
}

#[test]
fn update_scroll_offset_keeps_cursor_visible_when_scrolling_up() {
    assert_eq!(update_scroll_offset(5, 1, 3, 10), 1);
}
