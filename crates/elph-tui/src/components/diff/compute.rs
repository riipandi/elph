//! Diff computation using `similar::TextDiff::grouped_ops`.
//!
//! Produces structured hunks with line numbers, change counts, and
//! merge/separate decisions driven by the caller's `context_lines` parameter.

use similar::{ChangeTag, TextDiff};

use super::types::{DiffHunk, DiffHunkLine, DiffResult};

/// Compute a structured [`DiffResult`] from old/new text.
///
/// `context_lines` controls how many unchanged lines surround each change
/// region (default: 3, matching unified diff convention).
pub fn compute_diff(old_text: &str, new_text: &str, context_lines: usize) -> DiffResult {
    let diff = TextDiff::from_lines(old_text, new_text);
    let mut hunks: Vec<DiffHunk> = Vec::new();
    let mut added = 0usize;
    let mut removed = 0usize;

    let groups = diff.grouped_ops(context_lines);

    for group in &groups {
        let mut lines: Vec<DiffHunkLine> = Vec::new();
        let mut old_start = 1usize;
        let mut new_start = 1usize;
        let mut first = true;

        for op in group {
            for change in diff.iter_changes(op) {
                let tag = change.tag();
                let text = change.value().to_string();
                let old_lineno = change.old_index().map(|i| i + 1);
                let new_lineno = change.new_index().map(|i| i + 1);

                if first {
                    old_start = old_lineno.unwrap_or(1);
                    new_start = new_lineno.unwrap_or(1);
                    first = false;
                }

                match tag {
                    ChangeTag::Delete => removed += 1,
                    ChangeTag::Insert => added += 1,
                    ChangeTag::Equal => {}
                }

                lines.push(DiffHunkLine {
                    text,
                    old_lineno,
                    new_lineno,
                    tag,
                });
            }
        }

        if lines.is_empty() {
            continue;
        }

        let old_count = lines.iter().filter(|l| l.tag != ChangeTag::Insert).count();
        let new_count = lines.iter().filter(|l| l.tag != ChangeTag::Delete).count();

        hunks.push(DiffHunk {
            old_start,
            old_count,
            new_start,
            new_count,
            lines,
        });
    }

    DiffResult {
        old_path: None,
        new_path: None,
        hunks,
        added,
        removed,
    }
}

/// Set the old/new file paths on a [`DiffResult`] (for `--- a/…` / `+++ b/…` headers).
pub fn diff_result_with_paths(result: &mut DiffResult, old_path: Option<String>, new_path: Option<String>) {
    result.old_path = old_path;
    result.new_path = new_path;
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_diff_detects_additions_and_removals() {
        let result = compute_diff("a\nb\nc\n", "a\nx\nc\n", 1);
        assert_eq!(result.hunks.len(), 1);
        assert_eq!(result.added, 1);
        assert_eq!(result.removed, 1);
    }

    #[test]
    fn compute_diff_identical_text_yields_no_hunks() {
        let result = compute_diff("same\n", "same\n", 3);
        assert!(result.hunks.is_empty());
    }

    #[test]
    fn compute_diff_empty_to_empty() {
        let result = compute_diff("", "", 3);
        assert!(result.hunks.is_empty());
    }

    #[test]
    fn compute_diff_empty_old_is_entirely_added() {
        let result = compute_diff("", "new\ncontent\n", 3);
        assert!(!result.hunks.is_empty());
        assert_eq!(result.added, 2);
        assert_eq!(result.removed, 0);
    }

    #[test]
    fn compute_diff_empty_new_is_entirely_removed() {
        let result = compute_diff("old\ncontent\n", "", 3);
        assert!(!result.hunks.is_empty());
        assert_eq!(result.removed, 2);
        assert_eq!(result.added, 0);
    }

    #[test]
    fn compute_diff_merged_hunks_with_context() {
        let result = compute_diff("a\nb\nc\nd\ne\nf\n", "a\nX\nc\nY\ne\nf\n", 1);
        assert_eq!(result.hunks.len(), 1);
    }

    #[test]
    fn compute_diff_separate_hunks_with_gap() {
        let result = compute_diff("a\nb\nc\nd\ne\nf\ng\nh\ni\nj\n", "a\nX\nc\nd\nY\nf\ng\nh\ni\nj\n", 0);
        assert_eq!(result.hunks.len(), 2);
    }

    #[test]
    fn trivial_diff_result_totals() {
        let result = compute_diff("a\nb\nc\n", "a\nx\ny\nc\n", 1);
        assert_eq!(result.removed, 1);
        assert_eq!(result.added, 2);
    }

    #[test]
    fn diff_result_with_paths_sets_fields() {
        let mut result = compute_diff("old\n", "new\n", 1);
        diff_result_with_paths(&mut result, Some("a/old.rs".into()), Some("b/new.rs".into()));
        assert_eq!(result.old_path.as_deref(), Some("a/old.rs"));
        assert_eq!(result.new_path.as_deref(), Some("b/new.rs"));
    }

    #[test]
    fn diff_hunk_line_numbers_are_monotonic() {
        let result = compute_diff("line1\nline2\nline3\n", "line1\nmodified\nline3\n", 1);
        assert_eq!(result.hunks.len(), 1);
        let hunk = &result.hunks[0];

        let old_nums: Vec<Option<usize>> = hunk
            .lines
            .iter()
            .filter(|l| l.tag != ChangeTag::Insert)
            .map(|l| l.old_lineno)
            .collect();
        for w in old_nums.windows(2) {
            if let (Some(a), Some(b)) = (w[0], w[1]) {
                assert!(b > a, "old line numbers not monotonic: {:?}", old_nums);
            }
        }

        let new_nums: Vec<Option<usize>> = hunk
            .lines
            .iter()
            .filter(|l| l.tag != ChangeTag::Delete)
            .map(|l| l.new_lineno)
            .collect();
        for w in new_nums.windows(2) {
            if let (Some(a), Some(b)) = (w[0], w[1]) {
                assert!(b > a, "new line numbers not monotonic: {:?}", new_nums);
            }
        }
    }
}
