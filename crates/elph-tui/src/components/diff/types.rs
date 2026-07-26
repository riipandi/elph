//! Data model for diff hunks (grok-build inspired).

use similar::ChangeTag;

/// A single line within a diff hunk, with optional old/new line numbers.
#[derive(Clone, Debug, PartialEq)]
pub struct DiffHunkLine {
    /// The raw text content (including any trailing newline from the diff).
    pub text: String,
    /// Original-file line number (present for context and deleted lines).
    pub old_lineno: Option<usize>,
    /// New-file line number (present for context and inserted lines).
    pub new_lineno: Option<usize>,
    /// Whether this line is context, an addition, or a removal.
    pub tag: ChangeTag,
}

/// A contiguous hunk of diff lines with a header.
#[derive(Clone, Debug)]
pub struct DiffHunk {
    /// Old-file start line for the `@@ -old,count +new,count @@` header.
    pub old_start: usize,
    /// Number of old-file lines in this hunk.
    pub old_count: usize,
    /// New-file start line for the `@@ -old,count +new,count @@` header.
    pub new_start: usize,
    /// Number of new-file lines in this hunk.
    pub new_count: usize,
    /// The lines in this hunk (ordered as they appear in the diff).
    pub lines: Vec<DiffHunkLine>,
}

/// Complete diff result for one file comparison.
#[derive(Clone, Debug)]
pub struct DiffResult {
    /// Original-file header path (`--- a/…`).
    pub old_path: Option<String>,
    /// New-file header path (`+++ b/…`).
    pub new_path: Option<String>,
    /// Hunks in order of appearance.
    pub hunks: Vec<DiffHunk>,
    /// Total number of added lines across all hunks.
    pub added: usize,
    /// Total number of removed lines across all hunks.
    pub removed: usize,
}
