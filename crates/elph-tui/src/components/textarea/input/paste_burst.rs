//! Bracketed paste and raw key-burst paste handling.

use std::time::Instant;

use iocraft::prelude::*;

use super::super::state::TextareaState;
use super::TextareaInputResult;
use crate::paste::PasteBurstState;
use crate::paste::{
    extend_paste_submit_guard, paste_burst_append_key, paste_burst_begin_with_rewind, paste_burst_finish,
    paste_burst_live_document, paste_burst_reset,
};
use crate::text_editing::PASTE_SUBMIT_GUARD_WINDOW;
use crate::text_editing::paste_echo_guard_duration;

/// Keep the editor buffer in sync with the burst for short/normal typing streams.
///
/// Beyond this many buffered characters we skip per-key layout (true bulk paste) and
/// rely on idle merge to commit — but still flush on the idle timer so text is not lost.
const RAW_BURST_LIVE_SYNC_MAX_CHARS: usize = 48;

/// Commit an idle raw burst (gap since last key) before normal key dispatch.
pub(crate) fn merge_idle_burst(burst: &mut PasteBurstState, state: &mut TextareaState) -> bool {
    merge_burst_into_state(burst, state)
}

fn merge_burst_into_state(burst: &mut PasteBurstState, state: &mut TextareaState) -> bool {
    let Some((text, cursor)) = paste_burst_finish(burst) else {
        return false;
    };
    if state.text.len() > text.len() && state.text.starts_with(&text) {
        state.cursor = state.cursor.max(cursor).min(state.text.len());
    } else if state.text.len() == text.len() && state.text == text {
        state.cursor = state.cursor.max(cursor);
    } else if state.text != text {
        state.text = text;
        state.cursor = cursor;
    }
    state.vertical_col_preference = None;
    true
}

/// Apply the live burst document into the editor so rapid typing stays visible.
fn apply_live_burst_document(burst: &PasteBurstState, state: &mut TextareaState) -> bool {
    let Some((text, cursor)) = paste_burst_live_document(burst) else {
        return false;
    };
    if state.text == text && state.cursor == cursor {
        return false;
    }
    state.text = text;
    state.cursor = cursor.min(state.text.len());
    state.vertical_col_preference = None;
    true
}

/// Handle `TerminalEvent::Paste` (bracketed paste).
pub(crate) fn handle_bracketed_paste(
    data: &str,
    state: &mut TextareaState,
    burst: &mut PasteBurstState,
    last_key_at: &mut Option<Instant>,
) -> TextareaInputResult {
    paste_burst_reset(burst);
    state.apply_paste(data);
    *last_key_at = None;
    let now = Instant::now();
    let echo_guard = paste_echo_guard_duration(data.len());
    burst.suppress_raw_keys_until = Some(now + echo_guard);
    // Submit guard stays short — echo replay can last much longer for big pastes.
    extend_paste_submit_guard(burst, now, PASTE_SUBMIT_GUARD_WINDOW);
    TextareaInputResult::Changed
}

/// Key event context for raw paste burst handling.
pub(crate) struct RawBurstKey<'a> {
    pub code: KeyCode,
    pub kind: KeyEventKind,
    pub modifiers: KeyModifiers,
    pub now: Instant,
    pub in_burst: bool,
    pub state: &'a mut TextareaState,
    pub burst: &'a mut PasteBurstState,
    pub last_key_at: &'a mut Option<Instant>,
}

/// Raw paste burst from rapid key events (terminals without bracketed paste).
///
/// Returns `Some` when the key was fully handled; `None` to continue normal dispatch.
///
/// Short rapid streams (normal typing) keep the editor buffer live so characters never
/// disappear when the user pauses. Long streams skip per-key layout until idle merge.
pub(crate) fn handle_raw_burst_key(key: RawBurstKey<'_>) -> Option<TextareaInputResult> {
    if !key.in_burst {
        return None;
    }
    // Editing shortcuts (Ctrl/Cmd+Backspace, etc.) must not rewind/re-merge the burst —
    // that would swallow the first press and leave the buffer unchanged.
    if !crate::paste::raw_burst_accepts_key(key.code, key.kind, key.modifiers, true) {
        return None;
    }

    if !key.burst.active {
        // Don't start a burst when the cursor is mid-buffer — the burst
        // append model (anchor + splice) clobbers characters after the caret.
        // Let these keystrokes fall through to insert_char which does a
        // proper splice insert at the live cursor position.
        if key.state.cursor < key.state.text.len() {
            return None;
        }
        paste_burst_begin_with_rewind(key.burst, &key.state.text, key.state.cursor);
    }
    if paste_burst_append_key(key.burst, key.code, key.kind, key.modifiers, true) {
        *key.last_key_at = Some(key.now);
        let live_sync = key.burst.buffer.chars().count() <= RAW_BURST_LIVE_SYNC_MAX_CHARS;
        if live_sync {
            apply_live_burst_document(key.burst, key.state);
            return Some(TextareaInputResult::Changed);
        }
        // Bulk paste: buffer only; idle timer / next non-burst key commits.
        return Some(TextareaInputResult::Consumed);
    }
    if merge_burst_into_state(key.burst, key.state) {
        return Some(TextareaInputResult::Changed);
    }

    None
}
