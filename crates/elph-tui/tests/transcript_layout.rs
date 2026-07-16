use elph_tui::components::*;

#[test]
fn transcript_messages_revision_changes_when_content_changes() {
    let a = transcript_messages_revision(&[("hello", true)], 80);
    let b = transcript_messages_revision(&[("hello!", true)], 80);
    assert_ne!(a, b);
    let c = transcript_messages_revision(&[("hello", true)], 80);
    assert_eq!(a, c);
}

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
    let is_sticky_prompt = [false, true, false, true];
    assert_eq!(sticky_user_message_index(&layouts, &is_sticky_prompt, 0), None);
    assert_eq!(sticky_user_message_index(&layouts, &is_sticky_prompt, 1), None);
    assert_eq!(sticky_user_message_index(&layouts, &is_sticky_prompt, 2), Some(1));
    assert_eq!(sticky_user_message_index(&layouts, &is_sticky_prompt, 5), Some(1));
    assert_eq!(sticky_user_message_index(&layouts, &is_sticky_prompt, 6), Some(3));
}

#[test]
fn sticky_ignores_assistant_tool_and_plain_user() {
    let texts = ["assistant", "plain user", "submitted prompt", "tool call"];
    let layouts = layout_transcript_rows(&texts, 40, 1);
    let is_sticky_prompt = [false, false, true, false];
    assert_eq!(sticky_user_message_index(&layouts, &is_sticky_prompt, 20), Some(2));
    assert_eq!(sticky_user_message_index(&layouts, &[false, false, false, false], 20), None);
}

#[test]
fn transcript_text_width_reserves_bubble_padding() {
    assert_eq!(transcript_text_width(80), 77);
    assert_eq!(transcript_text_width(2), 1);
    assert_eq!(transcript_text_width(0), 1);
}

#[test]
fn transcript_bubble_inner_width_subtracts_horizontal_padding() {
    assert_eq!(transcript_bubble_inner_width(80, 1), 75);
    assert_eq!(transcript_bubble_inner_width(80, 0), 77);
}

#[test]
fn clamp_wrapped_transcript_lines_joins_soft_wrapped_rows() {
    let long = "word ".repeat(12);
    let (text, rows, truncated) = clamp_wrapped_transcript_lines(long.trim(), 20, 6);
    assert!(!truncated);
    assert!(rows > 1);
    assert!(text.contains('\n'));
    assert_eq!(text.matches('\n').count() + 1, rows as usize);
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
fn sticky_source_bubble_suppressed_only_when_viewport_overlaps() {
    let layouts = layout_transcript_rows(&["assistant", "user prompt"], 40, 1);
    let user_idx = 1;
    assert!(transcript_bubble_overlaps_viewport(&layouts, user_idx, 0, 8));
    assert_eq!(sticky_source_bubble_suppressed(&layouts, Some(user_idx), 0, 8), Some(user_idx));
    assert_eq!(sticky_source_bubble_suppressed(&layouts, Some(user_idx), 20, 4), None);
    assert_eq!(sticky_source_bubble_suppressed(&layouts, None, 0, 8), None);
}

#[test]
fn transcript_supports_sticky_scroll_requires_overflow() {
    let layouts = layout_transcript_rows(&["one", "two"], 40, 1);
    let rows = transcript_content_row_count(&layouts) as u16;
    assert!(rows >= 3);
    assert!(!transcript_supports_sticky_scroll(&layouts, rows));
    assert!(transcript_supports_sticky_scroll(&layouts, rows.saturating_sub(1)));
    assert!(!transcript_supports_sticky_scroll(&[], 20));
}

#[test]
fn active_sticky_uses_latest_when_auto_scroll_pinned() {
    let texts = ["sys", "user one", "assistant", "user two"];
    let layouts = layout_transcript_rows(&texts, 40, 1);
    let is_sticky_prompt = [false, true, false, true];
    let viewport = 5;
    let bottom_offset = 50;
    assert_eq!(
        active_sticky_user_message_index(&layouts, &is_sticky_prompt, bottom_offset, true, viewport),
        Some(3)
    );
    assert_eq!(
        active_sticky_user_message_index(&layouts, &is_sticky_prompt, 6, false, viewport),
        Some(3)
    );
}

#[test]
fn active_sticky_hidden_for_short_or_empty_transcript() {
    let texts = ["sys", "user one", "assistant", "user two"];
    let layouts = layout_transcript_rows(&texts, 40, 1);
    let is_sticky_prompt = [false, true, false, true];
    let tall_viewport = 40;
    assert_eq!(
        active_sticky_user_message_index(&layouts, &is_sticky_prompt, 0, true, tall_viewport),
        None
    );
    assert_eq!(active_sticky_user_message_index(&[], &[], 0, true, 20), None);
}

#[test]
fn active_sticky_hidden_at_scroll_top_during_manual_scroll() {
    let texts = ["sys", "user one"];
    let layouts = layout_transcript_rows(&texts, 40, 1);
    let is_sticky_prompt = [false, true];
    let viewport = 2;
    assert_eq!(sticky_user_message_index(&layouts, &is_sticky_prompt, 0), None);
    assert_eq!(
        active_sticky_user_message_index(&layouts, &is_sticky_prompt, 0, false, viewport),
        None
    );
}

#[test]
fn active_sticky_tracks_scrolled_past_prompt_while_manual_scrolling() {
    let texts = ["sys", "user one", "assistant", "user two"];
    let layouts = layout_transcript_rows(&texts, 40, 1);
    let is_sticky_prompt = [false, true, false, true];
    let viewport = 5;
    let first_user = 1usize;
    let second_user = 3usize;
    assert_eq!(
        active_sticky_user_message_index(&layouts, &is_sticky_prompt, 0, false, viewport),
        None
    );
    assert_eq!(
        active_sticky_user_message_index(
            &layouts,
            &is_sticky_prompt,
            layouts[first_user].start_row as i32,
            false,
            viewport
        ),
        Some(first_user)
    );
    assert_eq!(
        active_sticky_user_message_index(
            &layouts,
            &is_sticky_prompt,
            layouts[second_user].start_row as i32,
            false,
            viewport
        ),
        Some(second_user)
    );
}

#[test]
fn sticky_body_line_clamp_respects_panel_budget() {
    let pad = 2u16;
    assert_eq!(sticky_body_line_clamp(20, 3, pad), STICKY_MAX_BODY_ROWS);
    assert_eq!(sticky_body_line_clamp(8, 3, pad), STICKY_MAX_BODY_ROWS);
    assert_eq!(sticky_body_line_clamp(4, 3, pad), STICKY_MIN_BODY_ROWS);
}

#[test]
fn layout_sticky_header_shrinks_for_single_line_prompt() {
    let pad = 2u16;
    let header = layout_sticky_header("hello", 40, pad, 20, 3).expect("header");
    assert!(!header.truncated);
    assert_eq!(header.body_rows, 1);
    assert_eq!(header.height, sticky_header_display_rows(1, pad));
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
fn layout_sticky_header_sanitizes_dirty_prompt_before_clamp() {
    let dirty = "\x1b[1mExplain\x07\x1b[0m\r\nsticky\u{200b} scroll";
    let pad = 2u16;
    let header = layout_sticky_header(dirty, 40, pad, 20, 3).expect("header");
    assert!(!header.display_text.contains('\x1b'));
    assert!(!header.display_text.contains('\x07'));
    assert!(!header.display_text.contains('\u{200b}'));
    assert!(header.display_text.contains("Explain"));
    assert!(header.display_text.contains("sticky scroll"));
}

#[test]
fn layout_sticky_header_line_clamps_tall_prompt() {
    let long = "line\n".repeat(30);
    let sticky_pad = 2u16;
    let header = layout_sticky_header(&long, 40, sticky_pad, 20, 3).expect("header");
    assert!(header.truncated);
    assert!(header.height <= sticky_header_display_rows(STICKY_MAX_BODY_ROWS, sticky_pad));
    assert_eq!(header.display_text.matches('\n').count(), 1);
    assert!(header.display_text.contains('…'));
}

#[test]
fn pinned_bottom_offset_activates_latest_sticky_when_auto_scroll_pinned() {
    let texts = ["user paste"];
    let layouts = layout_transcript_rows(&texts, 40, 0);
    let is_sticky_prompt = [true];
    let pinned_offset = 80;
    assert_eq!(sticky_user_message_index(&layouts, &is_sticky_prompt, pinned_offset), Some(0));
    assert_eq!(
        active_sticky_user_message_index(&layouts, &is_sticky_prompt, pinned_offset, true, 20),
        None
    );

    let long = "word ".repeat(40);
    let long_layouts = layout_transcript_rows(&[long.trim()], 12, 0);
    let long_rows = transcript_content_row_count(&long_layouts) as u16;
    assert!(transcript_supports_sticky_scroll(&long_layouts, long_rows - 1));
    assert_eq!(
        active_sticky_user_message_index(&long_layouts, &[true], pinned_offset, true, long_rows - 1),
        Some(0)
    );
}
