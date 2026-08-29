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
fn display_text_for_row_range_slices_viewport() {
    let text = "alpha\nbeta gamma delta\nzeta";
    let layout = WrappedTextLayout::new_for_overlay_editor(text, 6);
    let slice = layout.display_text_for_row_range(text, 1, 2);
    assert!(!slice.is_empty());
    assert!(slice.lines().count() <= 2);
}

const ELPH_PASTE: &str = "**Elph** is a Rust workspace for AI agent applications: a coding agent CLI, shared agent runtime libraries, and terminal UI components. It is a port of the [pi](https://pi.dev) TypeScript ecosystem to Rust, with additional MCP (Model Context Protocol) support, native hooks, and an iocraft-based interactive TUI.";
const BULLET_PASTE: &str = "- **TOON Encoding** — Optional structured-data encoding for tool results (reduces token usage on tabular payloads).\n- **MCP** — Model Context Protocol client supporting stdio, streamable HTTP, and SSE transports with OAuth 2.1 and AES-256-GCM credential encryption.\n- **Agent** — `elph::agent` wraps `elph-agent`'s `AgentHarness` with session orchestration for the coding use case.\n";

fn assert_extend_matches_full(width: u16, base: &str, suffix: &str) {
    let wrap = width as usize;
    let mut text = base.to_string();
    let mut layout = WrappedTextLayout::new_for_overlay_editor(&text, width);
    for ch in suffix.chars() {
        let next = format!("{text}{ch}");
        let extended = WrappedTextLayout::try_extend_suffix(&layout, &text, &next, wrap)
            .unwrap_or_else(|| panic!("extend failed after char {ch:?}"));
        let full = WrappedTextLayout::new_for_overlay_editor(&next, width);
        assert_eq!(extended.row_count(), full.row_count(), "row_count after {ch:?}");
        assert_eq!(
            extended.row_column_for_offset(&next, next.len()),
            full.row_column_for_offset(&next, next.len()),
            "eof cursor after {ch:?}"
        );
        let scroll = extended.row_count().saturating_sub(3);
        assert_eq!(
            extended.display_text_for_row_range(&next, scroll, 4),
            full.display_text_for_row_range(&next, scroll, 4),
            "viewport slice after {ch:?}"
        );
        text = next;
        layout = extended;
    }
}

#[test]
fn try_extend_suffix_matches_full_layout() {
    assert_extend_matches_full(20, "", "hello world");
}

#[test]
fn try_extend_suffix_after_medium_paste() {
    assert_extend_matches_full(78, ELPH_PASTE, " and more typing");
    assert_extend_matches_full(40, ELPH_PASTE, " extra");
}

#[test]
fn try_extend_suffix_after_multiline_paste() {
    assert_extend_matches_full(40, BULLET_PASTE, "\n- new item");
}

#[test]
fn try_extend_suffix_after_paste_near_viewport_threshold() {
    let base = format!("{ELPH_PASTE}{}", "x".repeat(1800));
    assert_extend_matches_full(78, &base, "yz");
}

#[test]
fn display_text_for_row_range_at_eof_is_non_empty() {
    let text = "line one\n".repeat(80);
    let layout = WrappedTextLayout::new_for_overlay_editor(&text, 20);
    let scroll = layout.row_count().saturating_sub(5);
    let slice = layout.display_text_for_row_range(&text, scroll, 5);
    assert!(!slice.is_empty());
    assert!(slice.contains("line"));
}

#[test]
fn wrap_width_reserves_cursor_column() {
    assert_eq!(text_input_wrap_width(10), 9);
    assert_eq!(text_input_wrap_width(0), 0);
    assert_eq!(overlay_editor_wrap_width(10), 10);
}

#[test]
fn overlay_editor_eof_cursor_on_last_wrapped_row() {
    let text = "**Elph** is a Rust workspace for AI agent applications: a coding agent CLI, shared agent runtime libraries, and terminal UI components. It is a port of the [pi](https://pi.dev) TypeScript ecosystem to Rust, with additional MCP (Model Context Protocol) support, native hooks, and an iocraft-based interactive TUI.";
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
