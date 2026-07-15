//! Bracketed paste and raw key-burst paste handling.

use std::time::Instant;

use iocraft::prelude::*;

use super::super::state::TextareaState;
use super::TextareaInputResult;
use crate::paste::{
    PasteBurstState, extend_paste_submit_guard, paste_burst_append_key, paste_burst_begin_with_rewind,
    paste_burst_finish, paste_burst_live_document, paste_burst_reset, paste_live_cursor,
};
use crate::text_editing::{PASTE_SUBMIT_GUARD_WINDOW, paste_echo_guard_duration};

fn sync_burst_preview_full(burst: &PasteBurstState, state: &mut TextareaState) {
    let Some((text, cursor)) = paste_burst_live_document(burst) else {
        return;
    };
    state.text = text;
    state.cursor = cursor;
    state.vertical_col_preference = None;
}

/// Append one buffered codepoint without rebuilding the whole document when the live buffer matches.
fn try_incremental_burst_append(burst: &PasteBurstState, state: &mut TextareaState, ch: char) -> bool {
    let ch_len = ch.len_utf8();
    if burst.buffer.len() < ch_len {
        return false;
    }
    let target_len = paste_live_cursor(burst.anchor_cursor, &burst.buffer);
    let prev_len = target_len - ch_len;
    if state.text.len() != prev_len || state.cursor != prev_len {
        return false;
    }
    if state.text.len() < burst.anchor_cursor {
        return false;
    }
    if state.text[..burst.anchor_cursor] != burst.anchor_text {
        return false;
    }
    let buf_prefix = &burst.buffer[..burst.buffer.len() - ch_len];
    if buf_prefix != &state.text[burst.anchor_cursor..] {
        return false;
    }
    state.text.push(ch);
    state.cursor = target_len;
    state.vertical_col_preference = None;
    true
}

fn sync_burst_preview_to_state(burst: &PasteBurstState, state: &mut TextareaState, appended: Option<char>) {
    if let Some(ch) = appended
        && try_incremental_burst_append(burst, state, ch)
    {
        return;
    }
    sync_burst_preview_full(burst, state);
}

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

fn burst_append_char(code: KeyCode) -> Option<char> {
    match code {
        KeyCode::Char(c) => Some(c),
        KeyCode::Enter => Some('\n'),
        KeyCode::Tab => Some('\t'),
        _ => None,
    }
}

/// Raw paste burst from rapid key events (terminals without bracketed paste).
///
/// Returns `Some` when the key was fully handled; `None` to continue normal dispatch.
pub(crate) fn handle_raw_burst_key(key: RawBurstKey<'_>) -> Option<TextareaInputResult> {
    if !key.in_burst {
        return None;
    }

    if !key.burst.active {
        paste_burst_begin_with_rewind(key.burst, &key.state.text, key.state.cursor);
        sync_burst_preview_full(key.burst, key.state);
    }
    if paste_burst_append_key(key.burst, key.code, key.kind, key.modifiers, true) {
        let appended = burst_append_char(key.code);
        sync_burst_preview_to_state(key.burst, key.state, appended);
        *key.last_key_at = Some(key.now);
        return Some(TextareaInputResult::Changed);
    }
    if merge_burst_into_state(key.burst, key.state) {
        return Some(TextareaInputResult::Changed);
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paste::PasteBurstState;

    #[test]
    fn incremental_burst_append_avoids_full_rebuild() {
        let mut burst = PasteBurstState::default();
        burst.active = true;
        burst.anchor_text.clear();
        burst.anchor_cursor = 0;
        burst.buffer = "abc".into();

        let mut state = TextareaState::from_text("ab".into());
        state.cursor = 2;

        assert!(try_incremental_burst_append(&burst, &mut state, 'c'));
        assert_eq!(state.text, "abc");
        assert_eq!(state.cursor, 3);
    }
}
