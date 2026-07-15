use elph_tui::components::textarea::*;
use elph_tui::text_editing::{insert_newline_at_cursor, line_start_offset, wire_insert_newline};
use elph_tui::text_input_layout::{WrappedTextLayout, update_scroll_offset};

#[test]
fn insert_newline_at_cursor_appends() {
    let (text, next) = insert_newline_at_cursor("hi", 2);
    assert_eq!(text, "hi\n");
    assert_eq!(next, 3);
}

#[test]
fn resolve_suppressed_change_keeps_first_typed_char() {
    assert_eq!(resolve_suppressed_change("a".into()), "a");
}

#[test]
fn resolve_suppressed_change_drops_ghost_newlines() {
    assert_eq!(resolve_suppressed_change("\n".into()), "");
}

#[test]
fn unauthorized_newline_detects_plain_enter() {
    assert!(!is_unauthorized_newline_insert("hi", "hi"));
    assert!(is_unauthorized_newline_insert("hi", "hi\n"));
}

#[test]
fn pending_wire_newline_rejects_textinput_double_insert() {
    let prev = "hello\n\n";
    let textinput_ghost = "hello\n\n\n";
    assert!(is_unauthorized_newline_insert(prev, textinput_ghost));
    assert!(!is_unauthorized_newline_insert(prev, prev));
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
fn layout_cursor_ignores_trailing_newline_when_cursor_before_tail() {
    let text = "hello\nworld";
    assert_eq!(layout_cursor_for_viewport(text, 5), 5);
}

#[test]
fn plan_suppress_enter_clears_ghost_newline() {
    assert_eq!(
        plan_text_input_change("draft", 5, "\n", 5, true, false),
        PlannedTextInputChange::Suppressed {
            value: String::new(),
            reset_cursor: true,
        }
    );
}

#[test]
fn plan_suppress_enter_keeps_first_real_char() {
    assert_eq!(
        plan_text_input_change("draft", 5, "x", 5, true, false),
        PlannedTextInputChange::Suppressed {
            value: "x".into(),
            reset_cursor: false,
        }
    );
}

#[test]
fn plan_rollback_plain_enter_newline() {
    assert_eq!(
        plan_text_input_change("hi", 2, "hi\n", 2, false, false),
        PlannedTextInputChange::Rollback { cursor: 2 }
    );
}

#[test]
fn plan_keep_wire_value_on_pending_duplicate() {
    assert_eq!(
        plan_text_input_change("hello\n\n", 7, "hello\n\n\n", 8, false, true),
        PlannedTextInputChange::KeepWireNewline { cursor: 7 }
    );
}

#[test]
fn plan_accepts_normal_typing() {
    assert_eq!(
        plan_text_input_change("hi", 2, "hit", 2, false, false),
        PlannedTextInputChange::Accept { value: "hit".into() }
    );
}

#[test]
fn layout_textarea_reserves_scrollbar_column() {
    let layout = layout_textarea("one two three four five six seven", 0, 12, 1, Some(2));
    assert!(layout.show_scrollbar);
    assert_eq!(layout.input_width, 11);
}

#[test]
fn display_row_count_soft_wraps_long_lines() {
    assert_eq!(display_row_count("12345678901", 6), 3);
}

// Regression: first newline (empty row below / cursor stuck)

#[test]
fn first_newline_handle_on_newline_byte_omits_phantom_viewport_row() {
    let text = "hello\n";
    let handle_on_newline_byte = text.len().saturating_sub(1);
    assert_eq!(visible_row_count(text, handle_on_newline_byte, 20), 1);
}

#[test]
fn first_newline_snapshot_on_empty_row_expands_viewport() {
    let text = "hello\n";
    let snapshot = text.len();
    let layout_cursor = layout_cursor_for_viewport(text, snapshot);
    let layout = layout_textarea(text, layout_cursor, 20, 1, None);
    assert_eq!(layout.viewport_height, 2);
    assert_eq!(visible_row_count(text, layout_cursor, 20), 2);
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
fn cursor_sync_pushes_lagging_handle_to_snapshot_empty_row() {
    let text = "hello\n";
    let tail = text.len();
    assert_eq!(
        plan_cursor_sync(text, tail, tail.saturating_sub(1)),
        CursorSyncAction::PushHandleToSnapshot(tail)
    );
}

#[test]
fn cursor_sync_pulls_snapshot_when_user_moves_with_arrows() {
    assert_eq!(
        plan_cursor_sync("hello world", 2, 5),
        CursorSyncAction::PullSnapshotFromHandle(5)
    );
}

#[test]
fn cursor_sync_push_takes_priority_over_pull_on_empty_row() {
    assert_eq!(plan_cursor_sync("hello\n", 6, 3), CursorSyncAction::PushHandleToSnapshot(6));
}

#[test]
fn empty_continuation_row_is_last_wrapped_row() {
    let text = "hello\n";
    let layout = WrappedTextLayout::new(text, 20);
    assert_eq!(layout.row_column_for_offset(text.len()), (1, 0));
}

// Regression: second newline (double insert)

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
fn second_newline_rejects_textinput_triple_on_stale_buffer() {
    let wire_value = "hello\n\n";
    let wire_snapshot = wire_value.len();
    let textinput_stale_triple = "hello\n\n\n";
    assert_eq!(
        plan_text_input_change(wire_value, 7, textinput_stale_triple, wire_snapshot, false, true),
        PlannedTextInputChange::KeepWireNewline { cursor: 7 }
    );
}

#[test]
fn second_newline_in_sync_textinput_does_not_add_extra() {
    assert_eq!(
        plan_text_input_change("hello\n", 6, "hello\n\n", 7, false, true),
        PlannedTextInputChange::KeepWireNewline { cursor: 6 }
    );
}

#[test]
fn wire_then_matching_textinput_on_change_is_idempotent() {
    let prev = "hello\n";
    let (wire_text, snap) = wire_insert_newline(prev, prev.len());
    assert_eq!(wire_text, "hello\n\n");
    assert_eq!(
        plan_text_input_change(&wire_text, snap, &wire_text, snap, false, true),
        PlannedTextInputChange::Accept {
            value: wire_text.clone()
        }
    );
}

// Regression: plain Enter vs intentional newline

#[test]
fn plain_enter_ghost_newline_rolled_back_without_pending() {
    assert_eq!(
        plan_text_input_change("draft", 5, "draft\n", 5, false, false),
        PlannedTextInputChange::Rollback { cursor: 5 }
    );
}

#[test]
fn submit_suppression_clears_draft_after_send() {
    assert_eq!(
        plan_text_input_change("send me", 7, "send me\n", 7, true, false),
        PlannedTextInputChange::Suppressed {
            value: String::new(),
            reset_cursor: true,
        }
    );
}

#[test]
fn intentional_newline_accepted_when_wire_and_textinput_agree() {
    let (text, cursor) = wire_insert_newline("hi", 2);
    assert_eq!(text, "hi\n");
    assert_eq!(cursor, 3);
    assert_eq!(
        plan_text_input_change(&text, cursor, &text, cursor, false, false),
        PlannedTextInputChange::Accept { value: text }
    );
}

// Regression: arrow navigation on empty line after trailing newline

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
    let wrapped = WrappedTextLayout::new(text, layout.input_width);
    let layout_cursor = layout_cursor_for_viewport(text, text.len());
    let (row, _) = wrapped.row_column_for_offset(layout_cursor);
    let offset = update_scroll_offset(0, row, layout.viewport_height, layout.content_rows);
    assert!(row + 1 >= layout.viewport_height || offset <= row);
}
