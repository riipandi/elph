use elph_tui::components::ascii_font::{render_bitmap, render_figlet};
use elph_tui::components::card::CardBorderStyle;
use elph_tui::components::code::highlight_rust_line;
use elph_tui::components::diff::{diff_line_color, diff_line_prefix, side_by_side_lines, unified_lines};
use elph_tui::components::frame_buffer::FrameBuffer;
use elph_tui::components::markdown::render_markdown_lines;
use elph_tui::components::qr_code::render_qr;
use elph_tui::components::scroll_bar::{scroll_indicator_label, scrollbar_thumb_position, scrollbar_thumb_row_flags};
use elph_tui::components::scroll_box::{scroll_view_down, scroll_view_up};
use elph_tui::components::select::select_window_start;
use elph_tui::components::select::{select_key_delta, select_option_line};
use elph_tui::components::slider::slider_fill_percent;
use elph_tui::components::slider::{slider_key_delta, slider_label};
use elph_tui::components::tab_select::tab_select_key_to_index;
use elph_tui::components::textarea::compute_viewport_height;
use elph_tui::text_input_layout::WrappedTextLayout;
use iocraft::prelude::*;
use similar::ChangeTag;

#[test]
fn card_border_styles_map_to_iocraft() {
    assert_eq!(CardBorderStyle::Single.to_iocraft(), BorderStyle::Single);
    assert_eq!(CardBorderStyle::Double.to_iocraft(), BorderStyle::Double);
    assert_eq!(CardBorderStyle::Round.to_iocraft(), BorderStyle::Round);
    assert_eq!(CardBorderStyle::Bold.to_iocraft(), BorderStyle::Bold);
    assert_eq!(CardBorderStyle::None.to_iocraft(), BorderStyle::None);
    assert_eq!(CardBorderStyle::default(), CardBorderStyle::Single);
}

#[test]
fn diff_helpers_cover_all_tags() {
    assert_eq!(diff_line_prefix(ChangeTag::Delete), "- ");
    assert_eq!(diff_line_prefix(ChangeTag::Insert), "+ ");
    assert_eq!(diff_line_prefix(ChangeTag::Equal), "  ");
    assert!(matches!(diff_line_color(ChangeTag::Delete), Color::DarkRed));
    assert!(matches!(diff_line_color(ChangeTag::Insert), Color::DarkGreen));
    assert!(matches!(diff_line_color(ChangeTag::Equal), Color::DarkGrey));
}

#[test]
fn side_by_side_diff_handles_uneven_line_counts() {
    let lines = side_by_side_lines("one\ntwo", "alpha\nbeta\ngamma", 8);
    assert_eq!(lines.len(), 3);
}

#[test]
fn unified_diff_empty_inputs() {
    let lines = unified_lines("", "");
    assert!(lines.is_empty());
}

#[test]
fn markdown_renders_headings_code_and_lists() {
    let source = "# Title\n\nbody\n\n```rust\nlet x = 1;\n```\n\n- one\n- two";
    let lines = render_markdown_lines(source);
    assert!(!lines.is_empty());
}

#[test]
fn markdown_empty_source_yields_placeholder() {
    let lines = render_markdown_lines("");
    assert_eq!(lines.len(), 1);
}

#[test]
fn markdown_soft_and_hard_breaks() {
    let lines = render_markdown_lines("line one  \nline two");
    assert!(!lines.is_empty());
}

#[test]
fn select_window_start_empty_list() {
    assert_eq!(select_window_start(0, 5, 0), 0);
}

#[test]
fn select_window_start_clamps_to_end() {
    assert_eq!(select_window_start(99, 3, 5), 4);
}

#[test]
fn select_window_start_centers_selection() {
    assert_eq!(select_window_start(5, 5, 20), 3);
}

#[test]
fn slider_fill_percent_clamps_and_handles_degenerate_range() {
    assert_eq!(slider_fill_percent(5.0, 0.0, 10.0), 50.0);
    assert_eq!(slider_fill_percent(15.0, 0.0, 10.0), 100.0);
    assert_eq!(slider_fill_percent(-5.0, 0.0, 10.0), 0.0);
    assert_eq!(slider_fill_percent(1.0, 5.0, 5.0), 0.0);
}

#[test]
fn scrollbar_thumb_position_moves_with_offset() {
    assert_eq!(scrollbar_thumb_position(0, 10, 50), 0);
    assert!(scrollbar_thumb_position(20, 10, 50) > 0);
    assert_eq!(scrollbar_thumb_position(0, 10, 8), 0);
}

#[test]
fn scroll_indicator_label_formats_range() {
    assert_eq!(scroll_indicator_label(0, 10, 40), "1-10/40");
    assert_eq!(scroll_indicator_label(35, 10, 40), "36-40/40");
    assert_eq!(scroll_indicator_label(0, 0, 0), "1-0/1");
}

#[test]
fn compute_viewport_height_respects_bounds() {
    assert_eq!(compute_viewport_height(5, 2, None), 5);
    assert_eq!(compute_viewport_height(1, 3, None), 3);
    assert_eq!(compute_viewport_height(10, 2, Some(4)), 4);
    assert_eq!(compute_viewport_height(1, 5, Some(3)), 5);
}

#[test]
fn scroll_view_helpers_accept_default_handle() {
    let mut handle = ScrollViewHandle::default();
    scroll_view_up(&mut handle, 0);
    scroll_view_up(&mut handle, 2);
    scroll_view_down(&mut handle, 0);
    scroll_view_down(&mut handle, 2);
}

#[test]
fn render_bitmap_covers_glyph_variants() {
    for ch in ['A', 'B', 'E', 'L', 'P', 'H', 'Z', ' '] {
        let out = render_bitmap(&ch.to_string());
        assert!(!out.is_empty());
    }
}

#[test]
fn render_figlet_uses_standard_font_for_ascii() {
    let out = render_figlet("ELPH");
    assert!(!out.is_empty());
    assert!(out.lines().count() > 5);
}

#[test]
fn render_figlet_falls_back_to_bitmap_when_convert_fails() {
    let fallback = render_bitmap("🎉");
    let out = render_figlet("🎉");
    assert_eq!(out, fallback);
}

#[test]
fn frame_buffer_bounds_and_lines() {
    let mut buf = FrameBuffer::new(4, 2);
    buf.set_char(10, 10, 'x');
    buf.set_text(0, 0, "abcdextra");
    assert_eq!(buf.width(), 4);
    assert_eq!(buf.height(), 2);
    assert_eq!(buf.line(0), "abcd");
    assert_eq!(buf.line(1), "    ");
    assert_eq!(buf.line(5), "");
    assert_eq!(buf.lines().len(), 2);
    assert_eq!(FrameBuffer::default().width(), 0);
}

#[test]
fn highlight_rust_line_covers_comments_strings_and_symbols() {
    let comment = highlight_rust_line("  // note");
    assert!(!comment.is_empty());
    let string = highlight_rust_line(r#"let s = "hi";"#);
    assert!(!string.is_empty());
    let punct = highlight_rust_line("fn main() {");
    assert!(!punct.is_empty());
    let keyword = highlight_rust_line("pub fn foo()");
    assert!(!keyword.is_empty());
    let empty = highlight_rust_line("");
    assert_eq!(empty.len(), 1);
}

#[test]
fn render_qr_invalid_payload() {
    let out = render_qr(&"x".repeat(5000), "█", " ");
    assert_eq!(out, "invalid payload");
}

#[test]
fn wrapped_layout_handles_wide_chars_and_mid_row_offsets() {
    let text = "日本語テスト";
    let layout = WrappedTextLayout::new(text, 4);
    assert!(layout.row_count() >= 2);
    let (row, col) = layout.row_column_for_offset(text, 3);
    assert!(row < layout.row_count());
    let _ = col;
}

#[test]
fn scrollbar_thumb_row_flags_cover_overflow() {
    let flags = scrollbar_thumb_row_flags(10, 50, 5);
    assert_eq!(flags.len(), 10);
    assert!(flags.iter().any(|&on| on));
}

#[test]
fn scrollbar_thumb_row_flags_empty_when_content_fits() {
    assert!(scrollbar_thumb_row_flags(10, 8, 0).is_empty());
}

#[test]
fn select_option_line_with_and_without_description() {
    assert_eq!(select_option_line("> ", "Save", "file", true), "> Save\n   file");
    assert_eq!(select_option_line("  ", "Save", "file", false), "  Save");
}

#[test]
fn select_key_delta_handles_arrows_and_vim() {
    assert_eq!(select_key_delta(KeyCode::Down, false, 3), Some(1));
    assert_eq!(select_key_delta(KeyCode::Char('k'), true, 2), Some(-2));
    assert_eq!(select_key_delta(KeyCode::Enter, false, 1), None);
}

#[test]
fn tab_select_key_to_index_wraps() {
    assert_eq!(tab_select_key_to_index(1, 3, KeyCode::Right), 2);
    assert_eq!(tab_select_key_to_index(2, 3, KeyCode::Tab), 2);
    assert_eq!(tab_select_key_to_index(0, 3, KeyCode::Left), 0);
    assert_eq!(tab_select_key_to_index(1, 3, KeyCode::Char('h')), 0);
    assert_eq!(tab_select_key_to_index(1, 3, KeyCode::Char('l')), 2);
    assert_eq!(tab_select_key_to_index(2, 3, KeyCode::BackTab), 1);
    assert_eq!(tab_select_key_to_index(1, 3, KeyCode::Enter), 1);
    assert_eq!(tab_select_key_to_index(0, 0, KeyCode::Tab), 0);
}

#[test]
fn slider_key_delta_handles_arrows_and_vim() {
    assert_eq!(slider_key_delta(KeyCode::Right, 2.5), Some(2.5));
    assert_eq!(slider_key_delta(KeyCode::Char('h'), 1.0), Some(-1.0));
    assert_eq!(slider_key_delta(KeyCode::Enter, 1.0), None);
}

#[test]
fn slider_label_formats_or_empty() {
    assert_eq!(slider_label("Vol", 42.0), "Vol: 42");
    assert_eq!(slider_label("", 1.0), "");
}

#[test]
fn wrapped_layout_offset_past_end_clamps() {
    let text = "abc";
    let layout = WrappedTextLayout::new(text, 10);
    let (row, col) = layout.row_column_for_offset(text, 100);
    assert_eq!(row, 0);
    assert_eq!(col, 3);
}
