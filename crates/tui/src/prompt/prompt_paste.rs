use std::ops::Range;
use std::time::{Duration, Instant};

/// Minimum gap between keystrokes to end a paste burst.
pub const PASTE_BURST_GAP: Duration = Duration::from_millis(40);

/// Longer gap for treating Tab as pasted text instead of cycling agent mode.
pub const TAB_PASTE_GAP: Duration = Duration::from_millis(1000);

/// Collapse pastes with at least this many logical lines.
pub const PASTE_COLLAPSE_MIN_LINES: usize = 2;

/// Collapse pastes with at least this many bytes (single-line pastes stay expanded longer).
pub const PASTE_COLLAPSE_MIN_CHARS: usize = 256;

const MARKER_PREFIX: &str = "[Pasted: ";
const MARKER_SUFFIX: &str = " lines] ";

/// Tracks a run of recently inserted characters that may be a paste.
#[derive(Debug, Clone)]
pub struct PendingPaste {
    pub start: usize,
    pub end: usize,
    pub last_at: Instant,
}

impl PendingPaste {
    pub fn new(cursor_before: usize, cursor_after: usize, at: Instant) -> Self {
        Self {
            start: cursor_before,
            end: cursor_after,
            last_at: at,
        }
    }

    pub fn extend(&mut self, cursor_after: usize, at: Instant) {
        self.end = cursor_after;
        self.last_at = at;
    }

    pub fn tab_follows_paste(&self, at: Instant) -> bool {
        at.duration_since(self.last_at) < TAB_PASTE_GAP
    }

    pub fn slice<'a>(&self, text: &'a str) -> &'a str {
        &text[self.start..self.end.min(text.len())]
    }
}

/// Normalizes clipboard text for insertion (iocraft delivers char-by-char without `Event::Paste`).
pub fn normalize_paste_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\r' => {
                if chars.peek() != Some(&'\n') {
                    out.push('\n');
                }
            }
            ch => out.push(ch),
        }
    }
    out
}

/// Collapsed paste stored in the prompt field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollapsedPaste {
    pub full: String,
    pub summary: String,
}

impl CollapsedPaste {
    pub fn new(full: String, preview_width: usize) -> Self {
        let summary = format_paste_summary(&full, preview_width);
        Self { full, summary }
    }
}

/// Counts logical lines (minimum 1 for non-empty text).
pub fn line_count(text: &str) -> usize {
    if text.is_empty() {
        return 0;
    }
    text.chars().filter(|&c| c == '\n').count() + 1
}

/// Minimum pasted run length before a burst-ending Enter finalizes instead of submitting.
const PASTE_ENTER_FINALIZE_MIN_CHARS: usize = 3;

/// Returns `true` when Enter should finalize/collapse a pending paste instead of submitting.
pub fn enter_should_finalize_paste(
    text: &str,
    pending: Option<&PendingPaste>,
    paste_recent: bool,
    in_burst: bool,
) -> bool {
    let Some(run) = pending else {
        return false;
    };
    let slice = run.slice(text);
    if should_collapse_paste(slice) {
        return true;
    }
    paste_recent && in_burst && slice.len() >= PASTE_ENTER_FINALIZE_MIN_CHARS
}

/// Returns `true` when pasted text should collapse to a summary chip.
pub fn should_collapse_paste(text: &str) -> bool {
    line_count(text) >= PASTE_COLLAPSE_MIN_LINES || text.len() >= PASTE_COLLAPSE_MIN_CHARS
}

/// Builds `[Pasted: NN lines] preview` for display in the prompt field.
pub fn format_paste_summary(full: &str, preview_width: usize) -> String {
    let lines = line_count(full);
    let marker = format!("{MARKER_PREFIX}{lines:02}{MARKER_SUFFIX}");
    let preview_budget = preview_width.saturating_sub(marker.chars().count());
    let preview = first_line_preview(full, preview_budget);
    if preview.is_empty() {
        marker.trim_end().to_string()
    } else {
        format!("{marker}{preview}")
    }
}

/// Finalizes a pending paste run, collapsing it when large enough.
///
/// When the burst has ended but the pasted text is too small to collapse, returns `None` and
/// drops tracking. While the burst is still in flight, returns `Some(pending)` unchanged so the
/// full paste range stays intact.
pub fn finalize_pending_paste(
    pending: Option<PendingPaste>,
    text: &mut String,
    cursor: &mut usize,
    wrap_width: usize,
    pastes: &mut Vec<CollapsedPaste>,
    burst_ended: bool,
) -> Option<PendingPaste> {
    let Some(mut pending) = pending else {
        return None;
    };

    let raw = normalize_paste_text(pending.slice(text));
    let cursor_delta = replace_pending_slice(text, &mut pending, &raw);
    adjust_cursor_for_slice_replace(cursor, cursor_delta, pending.start, pending.end);

    if !should_collapse_paste(&raw) {
        return if burst_ended { None } else { Some(pending) };
    }

    let collapsed = CollapsedPaste::new(raw, wrap_width);
    let tail = text[pending.end..].to_string();
    text.truncate(pending.start);
    text.push_str(&collapsed.summary);
    *cursor = pending.start + collapsed.summary.len();
    text.push_str(&tail);
    pastes.push(collapsed);
    None
}

/// Expands collapsed paste summaries back to full text for submit.
pub fn expand_paste_markers(display: &str, pastes: &[CollapsedPaste]) -> String {
    let mut out = String::new();
    let mut rest = display;

    for paste in pastes {
        let Some(start) = rest.find(&paste.summary) else {
            continue;
        };
        out.push_str(&rest[..start]);
        out.push_str(&paste.full);
        rest = &rest[start + paste.summary.len()..];
    }

    out.push_str(rest);
    out
}

/// Finds the byte range of a collapsed paste summary covering `cursor`, if any.
pub fn paste_block_range(value: &str, cursor: usize, pastes: &[CollapsedPaste]) -> Option<Range<usize>> {
    let cursor = cursor.min(value.len());
    for paste in pastes {
        let mut search_from = 0usize;
        while let Some(start) = value[search_from..].find(&paste.summary) {
            let start = search_from + start;
            let end = start + paste.summary.len();
            if cursor >= start && cursor <= end {
                return Some(start..end);
            }
            search_from = end;
        }
    }
    None
}

/// Removes the collapsed paste block at `range` and returns the removed paste index (0-based).
pub fn remove_paste_block(value: &str, range: Range<usize>, pastes: &[CollapsedPaste]) -> (String, Option<usize>) {
    for (idx, paste) in pastes.iter().enumerate() {
        let mut search_from = 0usize;
        while let Some(start) = value[search_from..].find(&paste.summary) {
            let start = search_from + start;
            let end = start + paste.summary.len();
            if start == range.start && end == range.end {
                let mut next = String::new();
                next.push_str(&value[..range.start]);
                next.push_str(&value[range.end..]);
                return (next, Some(idx));
            }
            search_from = end;
        }
    }
    (value.to_string(), None)
}

/// Parsed collapsed paste marker for styled rendering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PasteDisplayMarker {
    pub start: usize,
    pub end: usize,
    pub label: String,
    pub preview: String,
}

/// Finds the first collapsed paste summary in `text`.
pub fn find_paste_marker_for_display(text: &str) -> Option<PasteDisplayMarker> {
    let start = text.find(MARKER_PREFIX)?;
    let after_prefix = &text[start + MARKER_PREFIX.len()..];
    let digits_end = after_prefix
        .char_indices()
        .take_while(|(_, ch)| ch.is_ascii_digit())
        .last()
        .map(|(idx, ch)| idx + ch.len_utf8())?;
    let after_digits = &after_prefix[digits_end..];
    if !after_digits.starts_with(MARKER_SUFFIX) {
        return None;
    }
    let label_end = start + MARKER_PREFIX.len() + digits_end + MARKER_SUFFIX.len();
    let label = text[start..label_end].to_string();
    let preview = text[label_end..].trim_end().to_string();
    Some(PasteDisplayMarker {
        start,
        end: label_end + preview.len(),
        label,
        preview,
    })
}

fn first_line_preview(full: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    let line = full
        .lines()
        .find(|line| !line.trim().is_empty())
        .or_else(|| full.lines().next())
        .unwrap_or("");
    let line = line.trim_end();
    if line.is_empty() {
        return String::new();
    }
    truncate_to_char_boundary(line, max_chars)
}

/// Replaces the pending paste byte range with normalized text, preserving surrounding content.
///
/// Returns the byte-length delta applied to the replaced slice (`new_len - old_len`).
fn replace_pending_slice(text: &mut String, pending: &mut PendingPaste, normalized: &str) -> isize {
    let old_len = pending.end.saturating_sub(pending.start);
    if old_len == normalized.len() && pending.slice(text) == normalized {
        return 0;
    }

    let old_end = pending.end;
    let tail = text[old_end..].to_string();
    text.truncate(pending.start);
    text.push_str(normalized);
    pending.end = pending.start + normalized.len();
    text.push_str(&tail);
    pending.end as isize - old_end as isize
}

fn adjust_cursor_for_slice_replace(cursor: &mut usize, delta: isize, range_start: usize, range_end: usize) {
    if delta == 0 || *cursor < range_start {
        return;
    }
    if *cursor >= range_end {
        *cursor = ((*cursor as isize + delta).max(0)) as usize;
    } else {
        let new_end = ((range_end as isize + delta).max(range_start as isize)) as usize;
        *cursor = new_end;
    }
}

fn truncate_to_char_boundary(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let mut out = String::new();
    for ch in text.chars().take(max_chars.saturating_sub(1)) {
        out.push(ch);
    }
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_windows_line_endings() {
        assert_eq!(normalize_paste_text("a\r\nb"), "a\nb");
        assert_eq!(normalize_paste_text("a\rb"), "a\nb");
    }

    #[test]
    fn collapses_multiline_paste() {
        assert!(should_collapse_paste("a\nb"));
        assert!(!should_collapse_paste("short"));
    }

    #[test]
    fn formats_zero_padded_line_count() {
        let summary = format_paste_summary("alpha\nbeta", 40);
        assert!(summary.starts_with("[Pasted: 02 lines] "));
        assert!(summary.contains("alpha"));
    }

    #[test]
    fn short_typed_prompt_does_not_finalize_on_enter() {
        let text = "hi".to_string();
        let pending = PendingPaste::new(0, text.len(), Instant::now());
        assert!(!enter_should_finalize_paste(&text, Some(&pending), false, false));
    }

    #[test]
    fn multiline_paste_finalizes_on_enter() {
        let text = "line one\nline two".to_string();
        let pending = PendingPaste::new(0, text.len(), Instant::now());
        assert!(enter_should_finalize_paste(&text, Some(&pending), true, true));
    }

    #[test]
    fn trailing_enter_after_rapid_paste_finalizes() {
        let text = "pasted text".to_string();
        let pending = PendingPaste::new(0, text.len(), Instant::now());
        assert!(enter_should_finalize_paste(&text, Some(&pending), true, true));
    }

    #[test]
    fn preview_preserves_leading_indentation() {
        let summary = format_paste_summary("    fn main() {\n        println!();\n    }", 50);
        assert!(summary.contains("    fn main()"));
    }

    #[test]
    fn finalizes_with_normalized_line_endings() {
        let body = format!("{}\r\n{}", "x".repeat(200), "y".repeat(50));
        let mut text = body.clone();
        let pending = PendingPaste::new(0, text.len(), Instant::now());
        let mut cursor = text.len();
        let mut pastes = Vec::new();
        finalize_pending_paste(Some(pending), &mut text, &mut cursor, 40, &mut pastes, true);
        assert!(!text.contains('\r'));
        assert_eq!(pastes[0].full, normalize_paste_text(&body));
    }

    #[test]
    fn finalizes_large_pending_paste() {
        let mut text = "prefix ".to_string();
        let start = text.len();
        text.push_str("alpha\nbeta");
        let pending = PendingPaste::new(start, text.len(), Instant::now());
        let mut cursor = text.len();
        let mut pastes = Vec::new();
        finalize_pending_paste(Some(pending), &mut text, &mut cursor, 40, &mut pastes, true);
        assert_eq!(pastes.len(), 1);
        assert!(text.contains("[Pasted: 02 lines]"));
        assert!(!text.contains("beta"));
    }

    #[test]
    fn expands_markers_on_submit() {
        let paste = CollapsedPaste::new("alpha\nbeta".into(), 40);
        let display = format!("before {} after", paste.summary);
        assert_eq!(expand_paste_markers(&display, &[paste]), "before alpha\nbeta after");
    }

    #[test]
    fn finds_paste_block_for_cursor() {
        let paste = CollapsedPaste::new("alpha\nbeta".into(), 40);
        let value = format!("hi {}", paste.summary);
        let range = paste_block_range(&value, 4, &[paste.clone()]).expect("cursor on marker");
        assert_eq!(&value[range], paste.summary);
    }

    #[test]
    fn burst_tracking_survives_slow_insertion_before_finalize() {
        let mut pending: Option<PendingPaste> = None;
        let mut text = String::new();
        let mut cursor = 0;
        let pasted = "fn main() {\n    println!(\"hi\");\n}";

        for ch in pasted.chars() {
            let cursor_before = cursor;
            text.insert(cursor, ch);
            cursor += ch.len_utf8();
            match pending.as_mut() {
                Some(run) => run.extend(cursor, Instant::now()),
                None => pending = Some(PendingPaste::new(cursor_before, cursor, Instant::now())),
            }
        }

        let mut pastes = Vec::new();
        let run = pending.take().expect("paste burst should be tracked");
        finalize_pending_paste(Some(run), &mut text, &mut cursor, 40, &mut pastes, true);
        assert!(text.contains("[Pasted: 03 lines]"));
        assert_eq!(pastes.len(), 1);
    }

    #[test]
    fn removes_paste_block_and_returns_index() {
        let paste = CollapsedPaste::new("alpha\nbeta".into(), 40);
        let value = format!("x {} y", paste.summary);
        let range = paste_block_range(&value, 3, &[paste.clone()]).unwrap();
        let (next, idx) = remove_paste_block(&value, range, &[paste]);
        assert_eq!(next, "x  y");
        assert_eq!(idx, Some(0));
    }
}
