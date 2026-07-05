//! Git-style content diff (Lumen `diff_algo.rs` patterns).

use similar::{ChangeTag, TextDiff};

/// Kind of change in a side-by-side diff row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeType {
    Equal,
    Insert,
    Delete,
    Modified,
}

/// Inline segment with optional emphasis (word-level diff).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlineSegment {
    pub text: String,
    pub emphasized: bool,
}

/// One row in a side-by-side diff view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffLine {
    pub old_line: Option<(usize, String)>,
    pub new_line: Option<(usize, String)>,
    pub change_type: ChangeType,
    pub old_segments: Option<Vec<InlineSegment>>,
    pub new_segments: Option<Vec<InlineSegment>>,
}

/// Default tab width for [`expand_tabs`].
pub const DEFAULT_TAB_WIDTH: usize = 8;
const MIN_UNCHANGED_RATIO: f64 = 0.20;

fn has_meaningful_content(s: &str) -> bool {
    s.chars().any(|c| !c.is_whitespace())
}

/// Expands tabs using [`DEFAULT_TAB_WIDTH`].
pub fn expand_tabs_default(text: &str) -> String {
    expand_tabs(text, DEFAULT_TAB_WIDTH)
}

/// Expands tabs to spaces for consistent display width.
pub fn expand_tabs(text: &str, tab_width: usize) -> String {
    let tab_width = tab_width.max(1);
    let mut out = String::with_capacity(text.len());
    let mut col = 0usize;
    for ch in text.chars() {
        match ch {
            '\t' => {
                let spaces = tab_width - (col % tab_width);
                out.extend(std::iter::repeat_n(' ', spaces));
                col += spaces;
            }
            ch => {
                out.push(ch);
                col += 1;
            }
        }
    }
    out
}

fn compute_word_diff(old_text: &str, new_text: &str) -> Option<(Vec<InlineSegment>, Vec<InlineSegment>)> {
    let diff = TextDiff::configure().diff_unicode_words(old_text, new_text);
    let mut old_segments = Vec::new();
    let mut new_segments = Vec::new();
    let mut unchanged_len = 0usize;

    for change in diff.iter_all_changes() {
        let text = change.value().to_string();
        match change.tag() {
            ChangeTag::Equal => {
                if has_meaningful_content(&text) {
                    unchanged_len += text.trim().len();
                }
                old_segments.push(InlineSegment {
                    text: text.clone(),
                    emphasized: false,
                });
                new_segments.push(InlineSegment {
                    text,
                    emphasized: false,
                });
            }
            ChangeTag::Delete => {
                old_segments.push(InlineSegment { text, emphasized: true });
            }
            ChangeTag::Insert => {
                new_segments.push(InlineSegment { text, emphasized: true });
            }
        }
    }

    let total_len = old_text.trim().len().max(new_text.trim().len());
    if total_len == 0 || (unchanged_len as f64 / total_len as f64) < MIN_UNCHANGED_RATIO {
        return None;
    }

    Some((old_segments, new_segments))
}

/// Count inserted and deleted lines without building the full side-by-side structure.
pub fn count_added_removed(old: &str, new: &str) -> (usize, usize) {
    let diff = TextDiff::from_lines(old, new);
    let mut added = 0usize;
    let mut removed = 0usize;
    for change in diff.iter_all_changes() {
        match change.tag() {
            ChangeTag::Insert => added += 1,
            ChangeTag::Delete => removed += 1,
            ChangeTag::Equal => {}
        }
    }
    (added, removed)
}

/// Computes a GitHub-style side-by-side diff with paired delete/insert rows.
pub fn compute_side_by_side(old: &str, new: &str, tab_width: usize) -> Vec<DiffLine> {
    let diff = TextDiff::from_lines(old, new);
    let mut lines = Vec::new();
    let mut old_num = 1usize;
    let mut new_num = 1usize;
    let changes: Vec<_> = diff.iter_all_changes().collect();
    let mut i = 0;

    while i < changes.len() {
        let change = &changes[i];
        match change.tag() {
            ChangeTag::Equal => {
                let text = expand_tabs(change.value().trim_end(), tab_width);
                lines.push(DiffLine {
                    old_line: Some((old_num, text.clone())),
                    new_line: Some((new_num, text)),
                    change_type: ChangeType::Equal,
                    old_segments: None,
                    new_segments: None,
                });
                old_num += 1;
                new_num += 1;
                i += 1;
            }
            ChangeTag::Delete => {
                let mut deletions = Vec::new();
                while i < changes.len() && changes[i].tag() == ChangeTag::Delete {
                    deletions.push((old_num, expand_tabs(changes[i].value().trim_end(), tab_width)));
                    old_num += 1;
                    i += 1;
                }

                let mut insertions = Vec::new();
                while i < changes.len() && changes[i].tag() == ChangeTag::Insert {
                    insertions.push((new_num, expand_tabs(changes[i].value().trim_end(), tab_width)));
                    new_num += 1;
                    i += 1;
                }

                let max_len = deletions.len().max(insertions.len());
                for j in 0..max_len {
                    let old_line = deletions.get(j).cloned();
                    let new_line = insertions.get(j).cloned();
                    let change_type = match (&old_line, &new_line) {
                        (Some(_), Some(_)) => ChangeType::Modified,
                        (Some(_), None) => ChangeType::Delete,
                        (None, Some(_)) => ChangeType::Insert,
                        (None, None) => unreachable!(),
                    };

                    let (old_segments, new_segments) = if matches!(change_type, ChangeType::Modified) {
                        let old_text = old_line.as_ref().map(|(_, t)| t.as_str()).unwrap_or("");
                        let new_text = new_line.as_ref().map(|(_, t)| t.as_str()).unwrap_or("");
                        compute_word_diff(old_text, new_text)
                            .map(|(o, n)| (Some(o), Some(n)))
                            .unwrap_or((None, None))
                    } else {
                        (None, None)
                    };

                    lines.push(DiffLine {
                        old_line,
                        new_line,
                        change_type,
                        old_segments,
                        new_segments,
                    });
                }
            }
            ChangeTag::Insert => {
                lines.push(DiffLine {
                    old_line: None,
                    new_line: Some((new_num, expand_tabs(change.value().trim_end(), tab_width))),
                    change_type: ChangeType::Insert,
                    old_segments: None,
                    new_segments: None,
                });
                new_num += 1;
                i += 1;
            }
        }
    }

    lines
}

/// Returns indices where a new hunk of changes begins.
pub fn find_hunk_starts(lines: &[DiffLine]) -> Vec<usize> {
    let mut hunks = Vec::new();
    let mut in_hunk = false;
    for (i, line) in lines.iter().enumerate() {
        let is_change = !matches!(line.change_type, ChangeType::Equal);
        if is_change && !in_hunk {
            hunks.push(i);
            in_hunk = true;
        } else if !is_change {
            in_hunk = false;
        }
    }
    hunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_additions_and_deletions() {
        let (added, removed) = count_added_removed("a\nb\n", "a\nc\n");
        assert_eq!(added, 1);
        assert_eq!(removed, 1);
    }

    #[test]
    fn pairs_modified_lines() {
        let lines = compute_side_by_side("foo\n", "bar\n", 8);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].change_type, ChangeType::Modified);
    }
}
