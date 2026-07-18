use elph_tui::components::scroll_bar::scrollbar_cell_char;
use elph_tui::components::scroll_box::{scroll_view_down_reaches_bottom, scroll_view_pinned_up_offset};
use elph_tui::components::select::{select_clamped_index, select_key_to_index, select_row_colors, select_row_prefix};
use elph_tui::components::slider::slider_key_to_value;
use elph_tui::components::tab_select::{tab_select_clamped_index, tab_select_tab_styles};
use elph_tui::prelude::*;
use elph_tui::text_editing::*;

#[test]
fn wire_edit_handle_key_applies_word_left() {
    let mut value = "hello world".to_string();
    let mut esc = false;
    let mut handle = TextInputHandle::default();
    let result =
        apply_wire_edit_key(KeyCode::Left, KeyEventKind::Press, KeyModifiers::ALT, false, false, &value, 11).unwrap();
    assert_eq!(result.cursor, 6);
    wire_edit_apply_result(result, &mut value, &mut handle, &mut esc);
    assert_eq!(value, "hello world");
}

#[test]
fn wire_edit_handle_key_applies_text_change() {
    let mut value = "hello world".to_string();
    let mut esc = false;
    let mut handle = TextInputHandle::default();
    let result = apply_wire_edit_key(
        KeyCode::Backspace,
        KeyEventKind::Press,
        KeyModifiers::SUPER,
        false,
        false,
        &value,
        11,
    )
    .unwrap();
    wire_edit_apply_result(result, &mut value, &mut handle, &mut esc);
    assert_eq!(value, "");
}

#[test]
fn wire_edit_apply_result_cursor_only() {
    let result = WireEditResult {
        text: "abc".into(),
        cursor: 1,
        pending_esc: false,
        cursor_only: true,
    };
    let mut value = "abc".to_string();
    let mut esc = true;
    let mut handle = TextInputHandle::default();
    wire_edit_apply_result(result, &mut value, &mut handle, &mut esc);
    assert_eq!(value, "abc");
    assert!(!esc);
}

#[test]
fn prev_word_skips_trailing_punctuation() {
    assert_eq!(prev_word_offset("hi!!!", 5), 0);
}

#[test]
fn delete_blank_line_above_content_line() {
    let text = "content\n\n";
    let cursor = "content\n\n".len();
    let (out, pos) = delete_to_line_start(text, cursor);
    assert_eq!(out, "content");
    assert_eq!(pos, "content".len());
}

#[test]
fn select_helpers_cover_row_and_key_paths() {
    assert_eq!(select_row_prefix(true), elph_tui::LIST_SELECTION_MARKER);
    assert_eq!(select_row_prefix(false), " ");
    let theme = elph_tui::components::UiTheme::default();
    assert_eq!(select_row_colors(theme, true), (theme.text_primary, Weight::Bold));
    assert_eq!(select_row_colors(theme, false), (theme.text_secondary, Weight::Normal));
    assert_eq!(select_clamped_index(9, 3), 2);
    assert_eq!(select_clamped_index(1, 0), 0);
    assert_eq!(select_key_to_index(1, 5, KeyCode::Down, KeyModifiers::SHIFT, 2), 3);
    assert_eq!(select_key_to_index(1, 5, KeyCode::Enter, KeyModifiers::empty(), 2), 1);
}

#[test]
fn slider_key_to_value_clamps() {
    assert_eq!(slider_key_to_value(0.0, 0.0, 10.0, KeyCode::Right, 3.0), 3.0);
    assert_eq!(slider_key_to_value(9.0, 0.0, 10.0, KeyCode::Right, 3.0), 10.0);
    assert_eq!(slider_key_to_value(5.0, 0.0, 10.0, KeyCode::Enter, 1.0), 5.0);
}

#[test]
fn tab_select_helpers_cover_styles() {
    let theme = elph_tui::components::UiTheme::default();
    assert_eq!(
        tab_select_tab_styles(theme, true),
        (BorderStyle::Round, theme.accent_soft, Weight::Bold)
    );
    assert_eq!(
        tab_select_tab_styles(theme, false),
        (BorderStyle::None, theme.text_muted, Weight::Normal)
    );
    assert_eq!(tab_select_clamped_index(4, 2), 1);
}

#[test]
fn scroll_view_offset_helpers() {
    assert_eq!(scroll_view_pinned_up_offset(50, 10, 2), 38);
    assert!(scroll_view_down_reaches_bottom(38, 50, 10, 2));
    assert!(!scroll_view_down_reaches_bottom(30, 50, 10, 2));
}

#[test]
fn scrollbar_cell_char_variants() {
    assert_eq!(scrollbar_cell_char(true), "\u{2503}");
    assert_eq!(scrollbar_cell_char(false), "\u{2502}");
}

#[test]
fn apply_wire_edit_key_noop_when_cursor_unchanged() {
    assert!(
        apply_wire_edit_key(KeyCode::Left, KeyEventKind::Press, KeyModifiers::ALT, false, false, "hi", 0,).is_none()
    );
}

#[test]
fn highlight_rust_single_token_line() {
    use elph_tui::components::UiTheme;
    use elph_tui::components::code::highlight_rust_line;
    assert!(!highlight_rust_line("foobar", UiTheme::default()).is_empty());
}

#[test]
fn utils_truncate_at_exact_width() {
    use elph_tui::utils::truncate_with_ellipsis;
    assert_eq!(truncate_with_ellipsis("abcd", 4), "abcd");
}

#[test]
fn next_word_skips_leading_punctuation_on_line() {
    assert_eq!(next_word_offset("!!!hi", 0), 3);
}

#[test]
fn delete_word_forward_no_op_at_eof_of_line() {
    let (text, cursor) = delete_word_forward("hello", 5);
    assert_eq!(text, "hello");
    assert_eq!(cursor, 5);
}

#[test]
fn color_rejects_short_hex() {
    use elph_tui::color::from_hex;
    assert_eq!(from_hex("#abc"), None);
}
