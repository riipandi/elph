use std::ops::Range;

use super::CollapsedPaste;
use super::offsets::{adjust_pastes_for_delete, reconcile_paste_offsets};

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
