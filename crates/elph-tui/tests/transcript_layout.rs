use elph_tui::components::*;

#[test]
fn layouts_accumulate_with_gap() {
    let layouts = layout_transcript_rows(&["a", "bb\ncc"], 20, 1);
    assert_eq!(layouts[0].start_row, 0);
    assert_eq!(layouts[0].row_count, 1);
    assert_eq!(layouts[1].start_row, 2);
    assert_eq!(layouts[1].row_count, 2);
}

#[test]
fn sticky_picks_last_user_at_or_above_offset() {
    let texts = ["sys", "user one", "assistant", "user two"];
    let layouts = layout_transcript_rows(&texts, 40, 1);
    let is_user = [false, true, false, true];
    assert_eq!(sticky_user_message_index(&layouts, &is_user, 0), None);
    assert_eq!(sticky_user_message_index(&layouts, &is_user, 1), None);
    assert_eq!(sticky_user_message_index(&layouts, &is_user, 2), Some(1));
    assert_eq!(sticky_user_message_index(&layouts, &is_user, 5), Some(1));
    assert_eq!(sticky_user_message_index(&layouts, &is_user, 6), Some(3));
}

#[test]
fn transcript_text_width_reserves_bubble_padding() {
    assert_eq!(transcript_text_width(80), 77);
    assert_eq!(transcript_text_width(2), 1);
    assert_eq!(transcript_text_width(0), 1);
}

#[test]
fn layout_transcript_rows_empty_input() {
    assert!(layout_transcript_rows(&[], 40, 1).is_empty());
}

#[test]
fn sticky_returns_none_on_length_mismatch() {
    let layouts = layout_transcript_rows(&["a"], 20, 0);
    assert_eq!(sticky_user_message_index(&layouts, &[], 3), None);
}

#[test]
fn effective_scroll_offset_pins_to_bottom() {
    assert_eq!(effective_scroll_offset(0, true, 50, 20), 30);
    assert_eq!(effective_scroll_offset(5, false, 50, 20), 5);
}

#[test]
fn effective_scroll_offset_when_content_fits() {
    assert_eq!(effective_scroll_offset(0, true, 10, 20), 0);
}
