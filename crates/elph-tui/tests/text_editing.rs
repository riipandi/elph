use std::time::{Duration, Instant};

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
fn delete_word_backward_uses_cursor_at_word_start() {
    let text = "hello world";
    let at_word_start = "hello ".len();
    let cursor = at_word_start;
    let (out, pos) = delete_word_backward(text, cursor);
    assert_eq!(out, "world");
    assert_eq!(pos, 0);
    let (wrong, _) = delete_word_backward(text, text.len());
    assert_eq!(wrong, "hello ");
}

#[test]
fn wire_edit_delete_word_syncs_handle_after_text_change() {
    let mut value = "hello world".to_string();
    let mut esc = false;
    let mut handle = TextInputHandle::default();
    let result = apply_wire_edit_key(
        KeyCode::Backspace,
        KeyEventKind::Press,
        KeyModifiers::ALT,
        false,
        false,
        &value,
        "hello ".len(),
    )
    .unwrap();
    wire_edit_apply_result(result, &mut value, &mut handle, &mut esc);
    assert_eq!(value, "world");
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
fn delete_to_line_start_at_buffer_start_is_noop() {
    let (out, pos) = delete_to_line_start("hello", 0);
    assert_eq!(out, "hello");
    assert_eq!(pos, 0);
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

#[test]
fn prev_word_at_line_start_stays() {
    assert_eq!(prev_word_offset("hello", 0), 0);
}

#[test]
fn prev_word_skips_punctuation_runs() {
    assert_eq!(prev_word_offset("one--two", 7), 5);
}

#[test]
fn next_word_skips_punctuation_runs() {
    assert_eq!(next_word_offset("one--two", 0), 5);
}

#[test]
fn delete_word_backward_no_op_without_preceding_word() {
    let (text, cursor) = delete_word_backward("hello", 0);
    assert_eq!(text, "hello");
    assert_eq!(cursor, 0);
}

#[test]
fn delete_word_forward_no_op_at_eof() {
    let (text, cursor) = delete_word_forward("hello", 5);
    assert_eq!(text, "hello");
    assert_eq!(cursor, 5);
}

#[test]
fn delete_to_line_end_no_op_when_already_at_line_end() {
    let (text, cursor) = delete_to_line_end("hello", 5);
    assert_eq!(text, "hello");
    assert_eq!(cursor, 5);
}

#[test]
fn delete_preceding_newline_when_cursor_at_line_start() {
    let text = "line one\nline two";
    let cursor = "line one\n".len();
    let (out, pos) = delete_word_backward(text, cursor);
    assert_eq!(out, "line oneline two");
    assert_eq!(pos, "line one".len());
}

#[test]
fn apply_wire_edit_key_word_left_moves_cursor_only() {
    let result = apply_wire_edit_key(
        KeyCode::Left,
        KeyEventKind::Press,
        KeyModifiers::ALT,
        false,
        false,
        "hello world",
        11,
    )
    .unwrap();
    assert_eq!(result.text, "hello world");
    assert_eq!(result.cursor, 6);
    assert!(result.cursor_only);
}

#[test]
fn apply_wire_edit_key_esc_sets_pending() {
    let result =
        apply_wire_edit_key(KeyCode::Esc, KeyEventKind::Press, KeyModifiers::empty(), false, false, "hi", 2).unwrap();
    assert!(result.pending_esc);
    assert_eq!(result.text, "hi");
}

#[test]
fn apply_wire_edit_key_release_is_ignored() {
    assert!(
        apply_wire_edit_key(
            KeyCode::Backspace,
            KeyEventKind::Release,
            KeyModifiers::ALT,
            false,
            false,
            "hi",
            2,
        )
        .is_none()
    );
}

#[test]
fn cursor_navigation_keys_are_classified_for_burst_suppression() {
    assert!(is_cursor_navigation_key(
        KeyCode::Up,
        KeyEventKind::Press,
        KeyModifiers::empty()
    ));
    assert!(!is_cursor_navigation_key(KeyCode::Up, KeyEventKind::Press, KeyModifiers::SHIFT));
    assert!(is_transcript_scroll_key(KeyCode::Up, KeyEventKind::Press, KeyModifiers::SHIFT));
    assert!(!is_cursor_navigation_key(
        KeyCode::Char('a'),
        KeyEventKind::Press,
        KeyModifiers::empty()
    ));
}

#[test]
fn paste_burst_detects_rapid_key_events() {
    let t0 = Instant::now();
    assert!(!key_event_in_paste_burst(None, t0));
    assert!(key_event_in_paste_burst(Some(t0), t0 + Duration::from_millis(10)));
    assert!(!key_event_in_paste_burst(Some(t0), t0 + Duration::from_millis(110)));
}

#[test]
fn should_submit_on_enter_skips_shift_and_raw_paste_burst() {
    let now = Instant::now();
    assert!(should_submit_on_enter(
        true,
        true,
        KeyCode::Enter,
        KeyEventKind::Press,
        KeyModifiers::empty(),
        false,
        None,
        now,
    ));
    assert!(!should_submit_on_enter(
        true,
        true,
        KeyCode::Enter,
        KeyEventKind::Press,
        KeyModifiers::SHIFT,
        false,
        None,
        now,
    ));
    assert!(!should_submit_on_enter(
        true,
        true,
        KeyCode::Enter,
        KeyEventKind::Press,
        KeyModifiers::empty(),
        true,
        None,
        now,
    ));
}

#[test]
fn should_submit_on_enter_blocked_during_paste_submit_guard() {
    let now = Instant::now();
    assert!(!should_submit_on_enter(
        true,
        true,
        KeyCode::Enter,
        KeyEventKind::Press,
        KeyModifiers::empty(),
        false,
        Some(now + Duration::from_millis(200)),
        now,
    ));
    assert!(should_submit_on_enter(
        true,
        true,
        KeyCode::Enter,
        KeyEventKind::Press,
        KeyModifiers::empty(),
        false,
        Some(now - Duration::from_millis(1)),
        now,
    ));
}

#[test]
fn wire_edit_cursor_only_must_not_touch_value_string() {
    let mut value = "hello world".to_string();
    let prev = value.clone();
    let mut esc = false;
    let mut handle = TextInputHandle::default();
    let result =
        apply_wire_edit_key(KeyCode::Left, KeyEventKind::Press, KeyModifiers::ALT, false, false, &value, 11).unwrap();
    wire_edit_apply_result(result, &mut value, &mut handle, &mut esc);
    assert_eq!(value, prev);
}

#[test]
fn wire_edit_text_change_updates_cursor() {
    let mut value = "hi".to_string();
    let mut cursor = 2;
    let mut esc = false;
    let result = apply_wire_edit_key(
        KeyCode::Enter,
        KeyEventKind::Press,
        KeyModifiers::SHIFT,
        true,
        false,
        &value,
        cursor,
    )
    .unwrap();
    wire_edit_apply_to_cursor(result, &mut value, &mut cursor, &mut esc);
    assert_eq!(value, "hi\n");
    assert_eq!(cursor, 3);
}

#[test]
fn apply_wire_edit_key_shift_enter_inserts_newline_at_eof() {
    let result =
        apply_wire_edit_key(KeyCode::Enter, KeyEventKind::Press, KeyModifiers::SHIFT, true, false, "hi", 2).unwrap();
    assert_eq!(result.text, "hi\n");
    assert_eq!(result.cursor, 3);
    assert!(!result.cursor_only);
}

#[test]
fn apply_wire_edit_key_shift_enter_splits_mid_line() {
    let result = apply_wire_edit_key(
        KeyCode::Enter,
        KeyEventKind::Press,
        KeyModifiers::SHIFT,
        true,
        false,
        "with session",
        4,
    )
    .unwrap();
    assert_eq!(result.text, "with\n session");
    assert_eq!(result.cursor, 5);
    assert!(!result.cursor_only);
}

#[test]
fn apply_wire_edit_key_super_backspace_deletes_line_start() {
    let result = apply_wire_edit_key(
        KeyCode::Backspace,
        KeyEventKind::Press,
        KeyModifiers::SUPER,
        false,
        false,
        "hello world",
        11,
    )
    .unwrap();
    assert_eq!(result.text, "");
    assert_eq!(result.cursor, 0);
    assert!(!result.cursor_only);
}

#[test]
fn apply_wire_edit_key_after_esc_word_left() {
    let result = apply_wire_edit_key(
        KeyCode::Left,
        KeyEventKind::Press,
        KeyModifiers::empty(),
        false,
        true,
        "hello world",
        11,
    )
    .unwrap();
    assert_eq!(result.cursor, 6);
    assert!(!result.pending_esc);
}

#[test]
fn apply_wire_edit_key_after_esc_falls_back_without_after_esc_match() {
    let result = apply_wire_edit_key(
        KeyCode::Char('q'),
        KeyEventKind::Press,
        KeyModifiers::empty(),
        false,
        true,
        "hi",
        1,
    );
    assert!(result.is_none());
}

#[test]
fn apply_wire_edit_key_all_edit_actions() {
    let del_word = apply_wire_edit_key(
        KeyCode::Backspace,
        KeyEventKind::Press,
        KeyModifiers::ALT,
        false,
        false,
        "hello world",
        11,
    )
    .unwrap();
    assert_eq!(del_word.text, "hello ");

    let del_fwd = apply_wire_edit_key(
        KeyCode::Delete,
        KeyEventKind::Press,
        KeyModifiers::ALT,
        false,
        false,
        "hello world",
        0,
    )
    .unwrap();
    assert_eq!(del_fwd.text, "world");

    let del_end = apply_wire_edit_key(
        KeyCode::Delete,
        KeyEventKind::Press,
        KeyModifiers::SUPER,
        false,
        false,
        "hello world",
        5,
    )
    .unwrap();
    assert_eq!(del_end.text, "hello");

    let word_right = apply_wire_edit_key(
        KeyCode::Right,
        KeyEventKind::Press,
        KeyModifiers::ALT,
        false,
        false,
        "hello world",
        0,
    )
    .unwrap();
    assert_eq!(word_right.cursor, 6);
}

#[test]
fn apply_wire_edit_key_unmatched_returns_none() {
    assert!(
        apply_wire_edit_key(
            KeyCode::Char('q'),
            KeyEventKind::Press,
            KeyModifiers::empty(),
            false,
            false,
            "hi",
            1,
        )
        .is_none()
    );
}

#[test]
fn apply_wire_edit_key_ctrl_j_inserts_newline() {
    let result = apply_wire_edit_key(
        KeyCode::Char('j'),
        KeyEventKind::Press,
        KeyModifiers::CONTROL,
        true,
        false,
        "hi",
        2,
    )
    .unwrap();
    assert_eq!(result.text, "hi\n");
    assert_eq!(result.cursor, 3);
}

#[test]
fn delete_to_line_start_on_middle_blank_run() {
    let text = "keep\n\n\n\nrest";
    let cursor = "keep\n\n\n\n".len();
    let (out, pos) = delete_to_line_start(text, cursor);
    assert_eq!(out, "keep\n\n\nrest");
    assert_eq!(pos, "keep\n\n".len() + 1);
}
