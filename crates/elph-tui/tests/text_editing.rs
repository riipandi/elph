use elph_tui::text_editing::*;
use iocraft::prelude::*;

#[test]
fn prev_word_from_end_of_second_word() {
    assert_eq!(prev_word_offset("hello world", 11), 6);
}

#[test]
fn prev_word_from_middle_of_word() {
    assert_eq!(prev_word_offset("hello world", 8), 6);
}

#[test]
fn prev_word_from_after_space() {
    assert_eq!(prev_word_offset("hello world", 5), 0);
}

#[test]
fn next_word_from_start() {
    assert_eq!(next_word_offset("hello world", 0), 6);
}

#[test]
fn delete_word_backward_removes_previous_word() {
    let (text, cursor) = delete_word_backward("hello world", 11);
    assert_eq!(text, "hello ");
    assert_eq!(cursor, 6);
}

#[test]
fn delete_to_line_start_on_second_line() {
    let text = "line one\nline two";
    let cursor = text.len();
    let (out, pos) = delete_to_line_start(text, cursor);
    assert_eq!(out, "line one\n");
    assert_eq!(pos, 9);
}

#[test]
fn prev_word_stays_on_same_line() {
    assert_eq!(prev_word_offset("hello\n", "hello\n".len()), 6);
    assert_eq!(prev_word_offset("hello\nworld", "hello\nworld".len()), 6);
}

#[test]
fn delete_word_backward_after_newline_keeps_first_line() {
    let text = "hello\nworld";
    let (out, cursor) = delete_word_backward(text, text.len());
    assert_eq!(out, "hello\n");
    assert_eq!(cursor, 6);
}

#[test]
fn delete_to_line_start_joins_empty_continuation_line() {
    let text = "hello\n";
    let (out, cursor) = delete_to_line_start(text, text.len());
    assert_eq!(out, "hello");
    assert_eq!(cursor, 5);
}

#[test]
fn delete_to_line_start_removes_all_trailing_blank_lines() {
    let text = "hello\n\n\n";
    let (out, cursor) = delete_to_line_start(text, text.len());
    assert_eq!(out, "hello");
    assert_eq!(cursor, 5);
}

#[test]
fn delete_to_line_start_removes_whitespace_only_blank_lines() {
    let text = "hello\n   \n";
    let (out, cursor) = delete_to_line_start(text, text.len());
    assert_eq!(out, "hello");
    assert_eq!(cursor, 5);
}

#[test]
fn delete_to_line_start_on_content_line_joins_one_line() {
    let text = "hello\n\nworld";
    let cursor = "hello\n\n".len();
    let (out, pos) = delete_to_line_start(text, cursor);
    assert_eq!(out, "hello\nworld");
    assert_eq!(pos, 6);
}

#[test]
fn delete_word_backward_joins_empty_continuation_line() {
    let text = "hello\n";
    let (out, cursor) = delete_word_backward(text, text.len());
    assert_eq!(out, "hello");
    assert_eq!(cursor, 5);
}

#[test]
fn delete_word_backward_joins_double_newline_at_empty_line() {
    let text = "hello\n\n";
    let (out, cursor) = delete_word_backward(text, text.len());
    assert_eq!(out, "hello\n");
    assert_eq!(cursor, 6);
}

#[test]
fn delete_word_backward_at_line_start_joins_with_previous_line() {
    let text = "hello\nworld";
    let cursor = "hello\n".len();
    let (out, pos) = delete_word_backward(text, cursor);
    assert_eq!(out, "helloworld");
    assert_eq!(pos, 5);
}

#[test]
fn match_ctrl_backspace() {
    let action = match_key_to_action(KeyCode::Backspace, KeyModifiers::CONTROL, false, false);
    assert_eq!(action, Some(TextEditAction::DeleteWordBackward));
}

#[test]
fn match_cmd_backspace() {
    let action = match_key_to_action(KeyCode::Backspace, KeyModifiers::SUPER, false, false);
    assert_eq!(action, Some(TextEditAction::DeleteToLineStart));
}

#[test]
fn match_macos_cmd_backspace_via_ctrl_u() {
    let action = match_key_to_action(KeyCode::Char('u'), KeyModifiers::CONTROL, false, false);
    assert_eq!(action, Some(TextEditAction::DeleteToLineStart));
}

#[test]
fn match_macos_cmd_delete_via_ctrl_k() {
    let action = match_key_to_action(KeyCode::Char('k'), KeyModifiers::CONTROL, false, false);
    assert_eq!(action, Some(TextEditAction::DeleteToLineEnd));
}

#[test]
fn match_alt_b_word_left() {
    let action = match_key_to_action(KeyCode::Char('b'), KeyModifiers::ALT, false, false);
    assert_eq!(action, Some(TextEditAction::WordLeft));
}

#[test]
fn match_alt_f_word_right() {
    let action = match_key_to_action(KeyCode::Char('f'), KeyModifiers::ALT, false, false);
    assert_eq!(action, Some(TextEditAction::WordRight));
}

#[test]
fn match_ctrl_j_inserts_newline() {
    let action = match_key_to_action(KeyCode::Char('j'), KeyModifiers::CONTROL, true, false);
    assert_eq!(action, Some(TextEditAction::InsertNewline));
}

#[test]
fn match_shift_enter_inserts_newline() {
    let action = match_key_to_action(KeyCode::Enter, KeyModifiers::SHIFT, true, false);
    assert_eq!(action, Some(TextEditAction::InsertNewline));
}

#[test]
fn plain_enter_is_not_newline_shortcut() {
    let action = match_key_to_action(KeyCode::Enter, KeyModifiers::empty(), true, false);
    assert_eq!(action, None);
}

#[test]
fn match_esc_then_left_word_left() {
    let action = match_key_to_action(KeyCode::Left, KeyModifiers::empty(), false, true);
    assert_eq!(action, Some(TextEditAction::WordLeft));
}

#[test]
fn utf8_word_boundaries() {
    assert_eq!(prev_word_offset("café résumé", "café résumé".len()), 6);
}

#[test]
fn line_start_and_end_offsets() {
    let text = "one\ntwo";
    assert_eq!(line_start_offset(text, 5), 4);
    assert_eq!(line_end_offset(text, 5), 7);
    assert_eq!(line_start_offset(text, 0), 0);
    assert_eq!(line_end_offset(text, 2), 3);
}

#[test]
fn is_word_char_alphanumeric_and_underscore() {
    assert!(is_word_char('a'));
    assert!(is_word_char('_'));
    assert!(!is_word_char(' '));
    assert!(!is_word_char('-'));
}

#[test]
fn insert_newline_in_middle_of_line() {
    let (text, cursor) = insert_newline_at_cursor("hello", 2);
    assert_eq!(text, "he\nllo");
    assert_eq!(cursor, 3);
}

#[test]
fn wire_insert_newline_append_places_cursor_past_trailing_newline() {
    let (text, cursor) = wire_insert_newline("hello", 5);
    assert_eq!(text, "hello\n");
    assert_eq!(cursor, 6);
    assert_eq!(cursor, text.len());
}

#[test]
fn wire_insert_newline_second_append_does_not_double() {
    let (t1, c1) = wire_insert_newline("hello\n", 6);
    assert_eq!(t1, "hello\n\n");
    assert_eq!(c1, 7);
    let (t2, c2) = wire_insert_newline(&t1, c1);
    assert_eq!(t2, "hello\n\n\n");
    assert_eq!(c2, 8);
}

#[test]
fn wire_insert_newline_middle_of_line_uses_byte_offset() {
    let (text, cursor) = wire_insert_newline("ab", 1);
    assert_eq!(text, "a\nb");
    assert_eq!(cursor, 2);
}

#[test]
fn wire_insert_newline_empty_buffer() {
    let (text, cursor) = wire_insert_newline("", 0);
    assert_eq!(text, "\n");
    assert_eq!(cursor, 1);
}

#[test]
fn apply_action_word_left() {
    let (text, cursor) = apply_action(TextEditAction::WordLeft, "hello world", 11);
    assert_eq!(text, "hello world");
    assert_eq!(cursor, 6);
}

#[test]
fn apply_action_insert_newline() {
    let (text, cursor) = apply_action(TextEditAction::InsertNewline, "ab", 1);
    assert_eq!(text, "a\nb");
    assert_eq!(cursor, 2);
}

#[test]
fn delete_word_forward_removes_next_word() {
    let (text, cursor) = delete_word_forward("hello world", 0);
    assert_eq!(text, "world");
    assert_eq!(cursor, 0);
}

#[test]
fn delete_to_line_end_removes_rest_of_line() {
    let text = "line one\nline two";
    let cursor = "line one\n".len();
    let (out, pos) = delete_to_line_end(text, cursor);
    assert_eq!(out, "line one\n");
    assert_eq!(pos, cursor);
}

#[test]
fn match_super_delete_deletes_to_line_end() {
    let action = match_key_to_action(KeyCode::Delete, KeyModifiers::SUPER, false, false);
    assert_eq!(action, Some(TextEditAction::DeleteToLineEnd));
}

#[test]
fn match_word_delete_forward() {
    let action = match_key_to_action(KeyCode::Delete, KeyModifiers::ALT, false, false);
    assert_eq!(action, Some(TextEditAction::DeleteWordForward));
}
