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

#[test]
fn scroll_viewport_height_reserves_sticky_header() {
    assert_eq!(scroll_viewport_height(20, 0), 20);
    assert_eq!(scroll_viewport_height(20, 8), 12);
    assert_eq!(scroll_viewport_height(3, 5), 1);
}

#[test]
fn sticky_header_row_count_includes_padding() {
    let layouts = layout_transcript_rows(&["one\n\ntwo"], 20, 0);
    assert_eq!(sticky_header_row_count(&layouts[0], 2), 5);
}

#[test]
fn clamp_sticky_header_preserves_min_scroll_area() {
    assert_eq!(clamp_sticky_header_rows(15, 20, 3), 15);
    assert_eq!(clamp_sticky_header_rows(18, 20, 3), 17);
    assert_eq!(clamp_sticky_header_rows(10, 3, 3), 0);
}

#[test]
fn active_sticky_none_when_auto_scroll_pinned() {
    let texts = ["sys", "user one", "assistant", "user two"];
    let layouts = layout_transcript_rows(&texts, 40, 1);
    let is_user = [false, true, false, true];
    let bottom_offset = 50;
    assert_eq!(active_sticky_user_message_index(&layouts, &is_user, bottom_offset, true), None);
    assert_eq!(active_sticky_user_message_index(&layouts, &is_user, 6, false), Some(3));
}

#[test]
fn sticky_body_line_clamp_respects_panel_budget() {
    assert_eq!(sticky_body_line_clamp(20, 3), 4);
    assert_eq!(sticky_body_line_clamp(8, 3), 4);
    assert_eq!(sticky_body_line_clamp(5, 3), 1);
}

#[test]
fn clamp_wrapped_transcript_lines_truncates_long_content() {
    let long = (0..12)
        .map(|i| format!("paragraph {i} with extra words"))
        .collect::<Vec<_>>()
        .join("\n");
    let (text, rows, truncated) = clamp_wrapped_transcript_lines(&long, 24, 3);
    assert!(truncated);
    assert_eq!(rows, 3);
    assert!(text.contains('…'));
    assert_eq!(text.matches('\n').count(), 2);
}

#[test]
fn clamp_wrapped_transcript_lines_keeps_short_content() {
    let (text, rows, truncated) = clamp_wrapped_transcript_lines("hi", 40, 4);
    assert!(!truncated);
    assert_eq!(rows, 1);
    assert_eq!(text, "hi");
}

#[test]
fn layout_sticky_header_line_clamps_tall_prompt() {
    let long = "line\n".repeat(30);
    let header = layout_sticky_header(&long, 40, 2, 20, 3).expect("header");
    assert!(header.truncated);
    assert!(header.height <= 7);
    assert!(header.display_text.contains('…'));
}

#[test]
fn pinned_bottom_offset_does_not_activate_sticky_when_auto_scroll_pinned() {
    let texts = ["user paste"];
    let layouts = layout_transcript_rows(&texts, 40, 0);
    let is_user = [true];
    let pinned_offset = 80;
    assert_eq!(sticky_user_message_index(&layouts, &is_user, pinned_offset), Some(0));
    assert_eq!(active_sticky_user_message_index(&layouts, &is_user, pinned_offset, true), None);
}
