use std::ops::Range;

use super::{CollapsedPaste, range_overlaps_used};

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
