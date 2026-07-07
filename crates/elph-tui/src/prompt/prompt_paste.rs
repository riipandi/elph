use super::paste_guard::PasteGuard;
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

/// Mutable paste-insert state for the pure char-by-char reducer.
#[derive(Debug, Clone, Default)]
pub struct PasteInsertCtx {
    pub text: String,
    pub cursor: usize,
    pub pending: Option<PendingPaste>,
    pub pastes: Vec<CollapsedPaste>,
    pub guard: PasteGuard,
    /// Minimum byte index for a new pending paste (byte after a typed separator space).
    pub separator_after: Option<usize>,
}

/// Computes the byte offset for a new [`PendingPaste`] without swallowing a typed separator.
pub fn pending_paste_start(
    guard: &PasteGuard,
    cursor_before: usize,
    separator_after: Option<usize>,
    now: Instant,
) -> usize {
    let was_in_burst = guard.is_in_burst(now);
    let mut start = cursor_before;
    if guard.is_paste_active(now) && was_in_burst && guard.rapid_insert_count() > 1 {
        start = guard.burst_run_start(cursor_before);
    }
    if let Some(min_start) = separator_after {
        start = start.max(min_start);
    }
    start
}

/// Inserts one pasted/typed character and updates pending-paste tracking.
pub fn apply_pasted_char_pure(ctx: &mut PasteInsertCtx, ch: char, now: Instant) {
    let was_in_burst = ctx.guard.is_in_burst(now);
    let paste_active_before = ctx.guard.is_paste_active(now);
    let prev_len = ctx.text.len();
    let cursor_before = ctx.cursor;

    if ctx.cursor == ctx.text.len() {
        ctx.text.push(ch);
    } else {
        ctx.text.insert(ctx.cursor, ch);
    }
    let inserted = ch.len_utf8();
    shift_paste_offsets_for_insert(&mut ctx.pastes, cursor_before, inserted);
    ctx.cursor += inserted;
    ctx.guard.record_change(prev_len, ctx.text.len(), now);

    if ch == ' ' && !was_in_burst && !paste_active_before {
        ctx.guard.reset_burst_chain();
        ctx.separator_after = Some(ctx.cursor);
    }

    let track_paste = ctx.guard.is_paste_active(now) || ctx.guard.is_in_burst(now);
    if track_paste {
        match ctx.pending.as_mut() {
            Some(run) => run.extend(ctx.cursor, now),
            None => {
                let start = pending_paste_start(&ctx.guard, cursor_before, ctx.separator_after, now);
                ctx.pending = Some(PendingPaste::new(start, ctx.cursor, now));
            }
        }
    } else {
        ctx.pending = None;
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
    /// Byte offset of `summary` in the display text at collapse time.
    pub offset: usize,
}

impl CollapsedPaste {
    pub fn new(full: String, preview_width: usize, offset: usize) -> Self {
        let summary = format_paste_summary(&full, preview_width);
        Self { full, summary, offset }
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
        // Keep the canonical suffix (trailing space) so display styling can parse the marker.
        marker
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
    let mut pending = pending?;

    let raw = normalize_paste_text(pending.slice(text));
    let cursor_delta = replace_pending_slice(text, &mut pending, &raw);
    adjust_cursor_for_slice_replace(cursor, cursor_delta, pending.start, pending.end);

    if !should_collapse_paste(&raw) {
        return if burst_ended { None } else { Some(pending) };
    }

    let collapsed = CollapsedPaste::new(raw, wrap_width, pending.start);
    let old_end = pending.end;
    let tail = text[pending.end..].to_string();
    text.truncate(pending.start);
    text.push_str(&collapsed.summary);
    *cursor = pending.start + collapsed.summary.len();
    text.push_str(&tail);
    let delta = collapsed.summary.len() as isize - (old_end.saturating_sub(pending.start) as isize);
    pastes.retain(|paste| paste.offset < pending.start || paste.offset >= old_end);
    shift_paste_offsets(pastes, old_end, delta);
    pastes.push(collapsed);
    None
}

/// Shifts stored offsets at or after `from` by `delta` bytes.
pub fn shift_paste_offsets(pastes: &mut [CollapsedPaste], from: usize, delta: isize) {
    if delta == 0 {
        return;
    }
    for paste in pastes.iter_mut() {
        if paste.offset >= from {
            paste.offset = ((paste.offset as isize) + delta).max(0) as usize;
        }
    }
}

/// Shifts offsets for an insertion at `at` with byte length `len`.
pub fn shift_paste_offsets_for_insert(pastes: &mut [CollapsedPaste], at: usize, len: usize) {
    if len > 0 {
        shift_paste_offsets(pastes, at, len as isize);
    }
}

/// Removes pastes wholly inside `range` and shifts later markers.
pub fn adjust_pastes_for_delete(pastes: &mut Vec<CollapsedPaste>, range: Range<usize>) {
    let len = range.end.saturating_sub(range.start);
    if len == 0 {
        return;
    }
    pastes.retain(|paste| {
        let end = paste.offset.saturating_add(paste.summary.len());
        end <= range.start || paste.offset >= range.end
    });
    shift_paste_offsets(pastes, range.end, -(len as isize));
}

/// Re-syncs stored offsets against the current display text (handles preceding edits).
pub fn reconcile_paste_offsets(display: &str, pastes: &mut [CollapsedPaste]) {
    let mut used: Vec<Range<usize>> = Vec::new();
    let mut order: Vec<usize> = (0..pastes.len()).collect();
    order.sort_by_key(|idx| pastes[*idx].offset);

    for idx in order {
        let summary = pastes[idx].summary.clone();
        let start = pastes[idx].offset;
        let end = start.saturating_add(summary.len());
        if end <= display.len()
            && display.get(start..end) == Some(summary.as_str())
            && !range_overlaps_used(start..end, &used)
        {
            used.push(start..end);
            continue;
        }

        let mut search = 0usize;
        let mut matched = false;
        while search < display.len() {
            let Some(rel) = display[search..].find(&summary) else {
                break;
            };
            let at = search + rel;
            let end = at.saturating_add(summary.len());
            if end <= display.len() && !range_overlaps_used(at..end, &used) {
                pastes[idx].offset = at;
                used.push(at..end);
                matched = true;
                break;
            }
            search = at.saturating_add(1);
        }
        if !matched {
            pastes[idx].offset = display.len();
        }
    }
}

/// Expands collapsed paste summaries back to full text for submit.
pub fn expand_paste_markers(display: &str, pastes: &[CollapsedPaste]) -> String {
    if pastes.is_empty() {
        return display.to_string();
    }

    let mut resolved = pastes.to_vec();
    reconcile_paste_offsets(display, &mut resolved);

    let mut ordered: Vec<&CollapsedPaste> = resolved.iter().collect();
    ordered.sort_by_key(|paste| paste.offset);

    let mut out = String::new();
    let mut cursor = 0usize;
    for paste in ordered {
        if paste.offset < cursor || paste.offset > display.len() {
            continue;
        }
        let end = paste.offset.saturating_add(paste.summary.len());
        if end > display.len() || display[paste.offset..end] != paste.summary {
            continue;
        }
        out.push_str(&display[cursor..paste.offset]);
        out.push_str(&paste.full);
        cursor = end;
    }
    out.push_str(&display[cursor..]);
    out
}

/// Finds the byte range of a collapsed paste summary covering `cursor`, if any.
pub fn paste_block_range(value: &str, cursor: usize, pastes: &[CollapsedPaste]) -> Option<Range<usize>> {
    let cursor = cursor.min(value.len());
    let mut resolved = pastes.to_vec();
    reconcile_paste_offsets(value, &mut resolved);

    for paste in &resolved {
        let start = paste.offset;
        let end = start.saturating_add(paste.summary.len());
        if end <= value.len() && value[start..end] == paste.summary && cursor >= start && cursor <= end {
            return Some(start..end);
        }
    }
    None
}

/// Removes the collapsed paste block at `range` and returns the removed paste index (0-based).
pub fn remove_paste_block(value: &str, range: Range<usize>, pastes: &[CollapsedPaste]) -> (String, Option<usize>) {
    if range.start > range.end || range.end > value.len() {
        return (value.to_string(), None);
    }
    let matched = &value[range.start..range.end];
    let mut resolved = pastes.to_vec();
    reconcile_paste_offsets(value, &mut resolved);
    let Some(idx) = resolved
        .iter()
        .position(|paste| paste.offset == range.start && paste.summary == matched)
    else {
        return (value.to_string(), None);
    };

    let mut next = String::new();
    next.push_str(&value[..range.start]);
    next.push_str(&value[range.end..]);
    (next, Some(idx))
}

/// Removes a paste block, updates `pastes` offsets, and returns the new display text.
pub fn remove_paste_block_and_adjust(
    value: &str,
    range: Range<usize>,
    pastes: &mut Vec<CollapsedPaste>,
) -> Option<String> {
    let (next, idx) = remove_paste_block(value, range.clone(), pastes);
    let idx = idx?;
    reconcile_paste_offsets(value, pastes);
    pastes.remove(idx);
    adjust_pastes_for_delete(pastes, range);
    Some(next)
}

fn range_overlaps_used(range: Range<usize>, used: &[Range<usize>]) -> bool {
    used.iter()
        .any(|other| range.start < other.end && other.start < range.end)
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
    let label_end = if after_digits.starts_with(MARKER_SUFFIX) {
        start + MARKER_PREFIX.len() + digits_end + MARKER_SUFFIX.len()
    } else if after_digits.starts_with(" lines]") {
        start + MARKER_PREFIX.len() + digits_end + " lines]".len()
    } else {
        return None;
    };
    let label = text[start..label_end].to_string();
    let preview_raw = &text[label_end..];
    let preview_end = preview_raw.find(MARKER_PREFIX).unwrap_or(preview_raw.len());
    let preview = preview_raw[..preview_end].trim_end().to_string();
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
    out.push_str("...");
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
    fn marker_preview_stops_before_next_chip() {
        let text = "[Pasted: 02 lines] alpha[Pasted: 02 lines] gamma";
        let marker = find_paste_marker_for_display(text).expect("first marker");
        assert_eq!(marker.preview, "alpha");
        let second = find_paste_marker_for_display(&text[marker.end..]).expect("second marker");
        assert_eq!(second.preview, "gamma");
    }

    #[test]
    fn narrow_summary_keeps_marker_suffix_for_styling() {
        let summary = format_paste_summary("alpha\nbeta", 18);
        assert!(summary.ends_with(" lines] "));
        assert!(find_paste_marker_for_display(&summary).is_some());
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
        let paste = CollapsedPaste::new("alpha\nbeta".into(), 40, 7);
        let display = format!("before {} after", paste.summary);
        assert_eq!(expand_paste_markers(&display, &[paste]), "before alpha\nbeta after");
    }

    #[test]
    fn expands_by_stored_offset_not_first_substring_match() {
        let summary = CollapsedPaste::new("real body".into(), 40, 0).summary;
        let offset = summary.len() + 1;
        let paste = CollapsedPaste::new("real body".into(), 40, offset);
        let display = format!("{summary} {summary}");
        assert_eq!(expand_paste_markers(&display, &[paste]), format!("{summary} real body"));
    }

    #[test]
    fn finds_paste_block_for_cursor() {
        let paste = CollapsedPaste::new("alpha\nbeta".into(), 40, 3);
        let value = format!("hi {}", paste.summary);
        let range = paste_block_range(&value, 4, std::slice::from_ref(&paste)).expect("cursor on marker");
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
        let paste = CollapsedPaste::new("alpha\nbeta".into(), 40, 2);
        let value = format!("x {} y", paste.summary);
        let range = paste_block_range(&value, 3, std::slice::from_ref(&paste)).unwrap();
        let (next, idx) = remove_paste_block(&value, range, &[paste]);
        assert_eq!(next, "x  y");
        assert_eq!(idx, Some(0));
    }

    #[test]
    fn removes_correct_index_when_duplicate_summaries() {
        let summary = CollapsedPaste::new("first body".into(), 40, 0).summary;
        let offset1 = summary.len() + 1;
        let paste0 = CollapsedPaste::new("first body".into(), 40, 0);
        let paste1 = CollapsedPaste {
            full: "second body".into(),
            summary: summary.clone(),
            offset: offset1,
        };
        let value = format!("{summary} {summary}");
        let mut pastes = vec![paste0, paste1];

        let range = paste_block_range(&value, offset1 + 2, &pastes).expect("cursor on second block");
        assert_eq!(range.start, offset1);

        let next = remove_paste_block_and_adjust(&value, range, &mut pastes).expect("removed");
        assert_eq!(next, format!("{summary} "));
        assert_eq!(pastes.len(), 1);
        assert_eq!(pastes[0].full, "first body");
        assert_eq!(pastes[0].offset, 0);
    }

    #[test]
    fn delete_first_paste_then_expand_remaining() {
        let summary = CollapsedPaste::new("first body".into(), 40, 0).summary;
        let offset1 = summary.len() + 1;
        let paste0 = CollapsedPaste::new("first body".into(), 40, 0);
        let paste1 = CollapsedPaste {
            full: "second body".into(),
            summary: summary.clone(),
            offset: offset1,
        };
        let value = format!("{summary} {summary}");
        let mut pastes = vec![paste0, paste1];

        let range = paste_block_range(&value, 1, &pastes).expect("cursor on first block");
        let next = remove_paste_block_and_adjust(&value, range, &mut pastes).expect("removed first");
        assert_eq!(next, format!(" {summary}"));
        assert_eq!(expand_paste_markers(&next, &pastes), " second body");
    }

    #[test]
    fn pre_edit_before_marker_then_expand() {
        let paste = CollapsedPaste::new("alpha\nbeta".into(), 40, 0);
        let mut pastes = vec![paste.clone()];
        let mut display = paste.summary.clone();
        display.insert_str(0, "EDIT");
        shift_paste_offsets_for_insert(&mut pastes, 0, "EDIT".len());
        assert_eq!(expand_paste_markers(&display, &pastes), "EDITalpha\nbeta");
    }

    #[test]
    fn pre_edit_before_marker_then_block_delete() {
        let paste = CollapsedPaste::new("alpha\nbeta".into(), 40, 0);
        let mut pastes = vec![paste.clone()];
        let mut display = paste.summary.clone();
        display.insert(0, 'X');
        shift_paste_offsets_for_insert(&mut pastes, 0, 1);
        let range = paste_block_range(&display, pastes[0].offset + 1, &pastes).expect("find block");
        let next = remove_paste_block_and_adjust(&display, range, &mut pastes).expect("delete");
        assert_eq!(next, "X");
        assert!(pastes.is_empty());
    }

    #[test]
    fn dual_collapsed_pastes_preserve_separator_space() {
        let t0 = Instant::now();
        let step = Duration::from_millis(1);
        let mut ctx = PasteInsertCtx::default();
        let wrap = 40;

        for (i, ch) in "alpha\nbeta".chars().enumerate() {
            apply_pasted_char_pure(&mut ctx, ch, t0 + step * (i as u32 + 1));
        }
        ctx.pending = finalize_pending_paste(
            ctx.pending.take(),
            &mut ctx.text,
            &mut ctx.cursor,
            wrap,
            &mut ctx.pastes,
            true,
        );
        ctx.guard.release_paste_active();

        apply_pasted_char_pure(&mut ctx, ' ', t0 + Duration::from_millis(50));

        for (i, ch) in "gamma\ndelta".chars().enumerate() {
            apply_pasted_char_pure(&mut ctx, ch, t0 + Duration::from_millis(51 + i as u64));
        }
        ctx.pending = finalize_pending_paste(
            ctx.pending.take(),
            &mut ctx.text,
            &mut ctx.cursor,
            wrap,
            &mut ctx.pastes,
            true,
        );

        assert!(
            ctx.text.contains("alpha [Pasted:"),
            "separator must stay between chips, got: {:?}",
            ctx.text
        );
        assert_eq!(ctx.pastes.len(), 2);
        assert_eq!(
            ctx.text.as_bytes().get(ctx.pastes[1].offset - 1),
            Some(&b' '),
            "byte before second chip must be a space, text={:?}",
            ctx.text
        );
    }

    #[test]
    fn adjust_pastes_for_delete_shifts_later_markers() {
        let first = CollapsedPaste::new("a\nb".into(), 40, 0);
        let gap = 1usize;
        let second = CollapsedPaste::new("c\nd".into(), 40, first.summary.len() + gap);
        let mut pastes = vec![first.clone(), second];
        adjust_pastes_for_delete(&mut pastes, 0..first.summary.len());
        assert_eq!(pastes.len(), 1);
        assert_eq!(pastes[0].offset, gap);
    }
}
