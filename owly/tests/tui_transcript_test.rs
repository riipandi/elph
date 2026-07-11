//! Integration tests for Owly tuie transcript formatting.

use owly::tui::transcript_render::entries_to_lines;
use owly::tui::{OwlyEntry, command_result_entry};

#[test]
fn banner_hint_and_user_share_transcript() {
    let entries = vec![
        OwlyEntry::hint("Owly v0.0.6"),
        OwlyEntry::user("init docs"),
        OwlyEntry::assistant("Starting init…"),
    ];
    let lines = entries_to_lines(&entries, true, false, &[]);
    assert!(lines.first().is_some_and(|l| l.contains("Owly")));
    assert!(lines.iter().any(|l| l.starts_with('❯')));
    assert!(lines.iter().any(|l| l.contains("Starting")));
}

#[test]
fn command_result_renders_checkmark() {
    let entries = vec![command_result_entry("done", true)];
    let lines = entries_to_lines(&entries, true, false, &[]);
    assert_eq!(lines.len(), 1);
    assert!(lines[0].starts_with('✓'));
}
