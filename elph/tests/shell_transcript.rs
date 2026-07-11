//! Integration tests for shell transcript rendering.

use elph::shell::{TranscriptRenderOptions, entries_to_lines, entries_to_lines_simple};
use elph_tui::{CollapseState, Theme, ToolExecutionState, ToolExecutionStatus, TranscriptEntry, strip_ansi};

fn plain_lines(lines: &[String]) -> Vec<String> {
    lines.iter().map(|line| strip_ansi(line)).collect()
}

#[test]
fn assistant_sections_separated_by_blank_line() {
    let entries = vec![
        TranscriptEntry::assistant("first"),
        TranscriptEntry::assistant("second"),
    ];
    let lines = plain_lines(&entries_to_lines_simple(&entries, true, false, &CollapseState::default()));
    assert_eq!(lines, vec!["first".to_string(), "second".to_string()]);
}

#[test]
fn user_turn_inserts_gap_before_next_role() {
    let entries = vec![
        TranscriptEntry::user("ask"),
        TranscriptEntry::assistant("answer"),
        TranscriptEntry::user("again"),
    ];
    let lines = plain_lines(&entries_to_lines_simple(&entries, true, false, &CollapseState::default()));
    assert!(lines.windows(2).any(|w| w[0] == "answer" && w[1].is_empty()));
    assert!(lines.iter().any(|l| l == "› again"));
}

#[test]
fn thinking_hidden_when_show_thinking_disabled() {
    let entry = TranscriptEntry::thinking("secret", false);
    let lines = entries_to_lines_simple(&[entry], false, false, &CollapseState::default());
    assert!(lines.is_empty());
}

#[test]
fn thinking_expanded_when_marked_in_collapse_state() {
    let entry = TranscriptEntry::thinking("internal plan", false);
    let mut collapse = CollapseState::default();
    collapse.toggle(0);
    let lines = entries_to_lines_simple(&[entry], true, false, &collapse);
    assert!(lines[0].starts_with('♦'));
    assert!(lines.iter().any(|l| l.contains("internal plan")));
}

#[test]
fn run_tool_labels_map_to_edit_and_run() {
    let edit = ToolExecutionState::new("1", "str_replace")
        .with_args("file.rs")
        .with_status(ToolExecutionStatus::Success);
    let run = ToolExecutionState::new("2", "bash")
        .with_args("cargo test")
        .with_status(ToolExecutionStatus::Running);

    let lines = entries_to_lines_simple(
        &[TranscriptEntry::tool(edit), TranscriptEntry::tool(run)],
        true,
        false,
        &CollapseState::default(),
    );
    assert!(lines.iter().any(|l| l.contains("Edit") && l.contains("file.rs")));
    assert!(lines.iter().any(|l| l.contains("Run") && l.contains("cargo test")));
}

#[test]
fn empty_user_message_produces_no_lines() {
    let lines = entries_to_lines_simple(&[TranscriptEntry::user("   \n  ")], true, false, &CollapseState::default());
    assert!(lines.is_empty());
}

#[test]
fn streaming_text_visible_while_agent_running() {
    let entries = vec![
        TranscriptEntry::user("go"),
        TranscriptEntry::assistant_streaming("streaming answer"),
    ];
    let collapse = CollapseState::default();
    let opts = TranscriptRenderOptions::new(true, true, &collapse, &[], 80, Theme::dark());
    let lines = entries_to_lines(&entries, &opts);
    assert!(lines.iter().any(|l| l.contains("streaming answer")));
}

#[test]
fn live_tools_surface_during_run() {
    let tool = ToolExecutionState::new("1", "bash")
        .with_args("make test")
        .with_status(ToolExecutionStatus::Running);
    let collapse = CollapseState::default();
    let tools = [tool];
    let opts = TranscriptRenderOptions::new(false, true, &collapse, &tools, 80, Theme::dark());
    let lines = entries_to_lines(&[TranscriptEntry::user("test")], &opts);
    assert!(lines.iter().any(|l| l.contains("make test")));
}

#[test]
fn completed_run_includes_streaming_assistant() {
    let entries = vec![
        TranscriptEntry::user("go"),
        TranscriptEntry::assistant_streaming("partial"),
        TranscriptEntry::assistant("final"),
    ];
    let lines = entries_to_lines_simple(&entries, true, false, &CollapseState::default());
    assert!(lines.iter().any(|l| l.contains("partial")));
    assert!(lines.iter().any(|l| l.contains("final")));
}
