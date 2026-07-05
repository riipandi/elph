use elph_tui::{ChangeType, compute_side_by_side, count_added_removed, find_hunk_starts};
use similar_asserts::assert_eq;

#[test]
fn count_added_removed_reports_line_changes() {
    let (added, removed) = count_added_removed("alpha\nbeta\n", "alpha\ngamma\n");
    assert_eq!(added, 1);
    assert_eq!(removed, 1);
}

#[test]
fn side_by_side_pairs_replacement_as_modified() {
    let lines = compute_side_by_side("foo\n", "bar\n", 8);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].change_type, ChangeType::Modified);
    assert_eq!(lines[0].old_line.as_ref().map(|(n, _)| *n), Some(1));
    assert_eq!(lines[0].new_line.as_ref().map(|(n, _)| *n), Some(1));
}

#[test]
fn side_by_side_handles_pure_insertion() {
    let lines = compute_side_by_side("a\n", "a\nb\n", 8);
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0].change_type, ChangeType::Equal);
    assert_eq!(lines[1].change_type, ChangeType::Insert);
    assert!(lines[1].old_line.is_none());
}

#[test]
fn side_by_side_handles_pure_deletion() {
    let lines = compute_side_by_side("a\nb\n", "a\n", 8);
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0].change_type, ChangeType::Equal);
    assert_eq!(lines[1].change_type, ChangeType::Delete);
    assert!(lines[1].new_line.is_none());
}

#[test]
fn word_diff_emphasizes_changed_segments_on_modified_lines() {
    let lines = compute_side_by_side("hello world\n", "hello there\n", 8);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].change_type, ChangeType::Modified);
    let old_segments = lines[0].old_segments.as_ref().expect("old segments");
    let new_segments = lines[0].new_segments.as_ref().expect("new segments");
    assert!(old_segments.iter().any(|s| s.emphasized));
    assert!(new_segments.iter().any(|s| s.emphasized));
}

#[test]
fn find_hunk_starts_marks_change_boundaries() {
    let lines = compute_side_by_side("a\nb\nc\n", "a\nB\nc\n", 8);
    let hunks = find_hunk_starts(&lines);
    assert_eq!(hunks, vec![1]);
}
