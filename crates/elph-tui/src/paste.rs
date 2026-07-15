//! Multiline paste helpers (bracketed paste + raw burst fallback).

use iocraft::prelude::*;

pub fn newline_count(text: &str) -> usize {
    text.chars().filter(|&c| c == '\n').count()
}

/// Normalize clipboard text to Unix newlines for the editor buffer.
pub fn normalize_paste_text(raw: &str) -> String {
    if !raw.contains('\r') {
        return raw.to_string();
    }
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\r' {
            if chars.peek() == Some(&'\n') {
                chars.next();
            }
            out.push('\n');
        } else {
            out.push(ch);
        }
    }
    out
}

/// Insert `paste` at `cursor` (byte offset) and return `(text, new_cursor)`.
pub fn apply_paste_at_cursor(text: &str, cursor: usize, paste: &str) -> (String, usize) {
    let cursor = cursor.min(text.len());
    let normalized = normalize_paste_text(paste);
    if normalized.is_empty() {
        return (text.to_string(), cursor);
    }
    if cursor == text.len() {
        let mut out = String::with_capacity(text.len() + normalized.len());
        out.push_str(text);
        out.push_str(&normalized);
        let end = out.len();
        return (out, end);
    }
    let mut out = String::with_capacity(text.len() + normalized.len());
    out.push_str(&text[..cursor]);
    out.push_str(&normalized);
    out.push_str(&text[cursor..]);
    (out, cursor + normalized.len())
}

/// Live cursor after appending `suffix` to `anchor` at `anchor_cursor`.
pub fn paste_live_cursor(anchor_cursor: usize, suffix: &str) -> usize {
    anchor_cursor + suffix.len()
}

/// State for terminals that deliver paste as a rapid key burst (no bracketed paste).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PasteBurstState {
    pub active: bool,
    pub anchor_text: String,
    pub anchor_cursor: usize,
    pub buffer: String,
    /// Suppress raw key replay until this instant (after bracketed paste).
    pub suppress_raw_keys_until: Option<std::time::Instant>,
    /// Suppress plain-Enter submit until this instant (trailing paste echo).
    pub suppress_submit_until: Option<std::time::Instant>,
}

fn take_burst_work(state: &mut PasteBurstState) -> (Option<std::time::Instant>, Option<std::time::Instant>) {
    let raw = state.suppress_raw_keys_until;
    let submit = state.suppress_submit_until;
    state.active = false;
    state.anchor_text.clear();
    state.anchor_cursor = 0;
    state.buffer.clear();
    (raw, submit)
}

fn restore_guards(state: &mut PasteBurstState, raw: Option<std::time::Instant>, submit: Option<std::time::Instant>) {
    state.suppress_raw_keys_until = raw;
    state.suppress_submit_until = submit;
}

/// Extend submit guard from `now` without shortening an existing longer deadline.
pub fn extend_paste_submit_guard(state: &mut PasteBurstState, now: std::time::Instant, window: std::time::Duration) {
    let deadline = now + window;
    state.suppress_submit_until = Some(match state.suppress_submit_until {
        Some(existing) => existing.max(deadline),
        None => deadline,
    });
}

pub fn paste_burst_begin(state: &mut PasteBurstState, text: &str, cursor: usize) {
    state.active = true;
    state.anchor_text = text.to_string();
    state.anchor_cursor = cursor.min(text.len());
    state.buffer.clear();
}

/// Begin a raw burst, rewinding the last typed character when it was already applied as normal
/// typing before the burst was detected (second rapid key).
pub fn paste_burst_begin_with_rewind(state: &mut PasteBurstState, text: &str, cursor: usize) {
    let cursor = cursor.min(text.len());
    if cursor == 0 {
        paste_burst_begin(state, text, cursor);
        return;
    }
    let rewind = text[..cursor]
        .chars()
        .last()
        .map(|c| cursor.saturating_sub(c.len_utf8()))
        .unwrap_or(cursor);
    state.active = true;
    state.anchor_text = text[..rewind].to_string();
    state.anchor_cursor = rewind;
    state.buffer = text[rewind..cursor].to_string();
}

/// Append one pasteable key to an active burst. Returns false for non-paste keys.
pub fn paste_burst_append_key(
    state: &mut PasteBurstState,
    code: KeyCode,
    kind: KeyEventKind,
    modifiers: KeyModifiers,
    multiline: bool,
) -> bool {
    if kind == KeyEventKind::Release {
        return false;
    }
    if modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::META) {
        return false;
    }
    match code {
        KeyCode::Char(c) => {
            state.buffer.push(c);
            true
        }
        KeyCode::Enter if multiline => {
            state.buffer.push('\n');
            true
        }
        KeyCode::Tab => {
            state.buffer.push('\t');
            true
        }
        _ => false,
    }
}

/// Live document while a raw burst is in progress (anchor + buffered keys).
pub fn paste_burst_live_document(burst: &PasteBurstState) -> Option<(String, usize)> {
    if !burst.active {
        return None;
    }
    if burst.buffer.is_empty() {
        return Some((burst.anchor_text.clone(), burst.anchor_cursor));
    }
    let cursor = paste_live_cursor(burst.anchor_cursor, &burst.buffer);
    if burst.anchor_cursor == burst.anchor_text.len() {
        let mut text = String::with_capacity(burst.anchor_text.len() + burst.buffer.len());
        text.push_str(&burst.anchor_text);
        text.push_str(&burst.buffer);
        return Some((text, cursor));
    }
    Some(apply_paste_at_cursor(&burst.anchor_text, burst.anchor_cursor, &burst.buffer))
}

/// Finish a burst and return the merged document, or `None` if inactive/empty.
pub fn paste_burst_finish(state: &mut PasteBurstState) -> Option<(String, usize)> {
    if !state.active {
        return None;
    }
    let anchor_text = state.anchor_text.clone();
    let anchor_cursor = state.anchor_cursor;
    let buffer = std::mem::take(&mut state.buffer);
    let (raw_guard, submit_guard) = take_burst_work(state);
    restore_guards(state, raw_guard, submit_guard);
    if buffer.is_empty() {
        return None;
    }
    Some(apply_paste_at_cursor(&anchor_text, anchor_cursor, &buffer))
}

/// Discard an in-progress raw burst without merging into the document.
pub fn paste_burst_reset(state: &mut PasteBurstState) {
    let (raw_guard, submit_guard) = take_burst_work(state);
    restore_guards(state, raw_guard, submit_guard);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_paste_text_unifies_newlines() {
        assert_eq!(normalize_paste_text("a\r\nb\rc"), "a\nb\nc");
    }

    #[test]
    fn apply_paste_at_cursor_inserts_at_offset() {
        assert_eq!(apply_paste_at_cursor("hi", 2, " there"), ("hi there".into(), 8));
    }

    #[test]
    fn live_document_tracks_in_progress_burst() {
        let mut state = PasteBurstState::default();
        paste_burst_begin_with_rewind(&mut state, "#", 1);
        assert_eq!(paste_burst_live_document(&state), Some(("#".into(), 1)));
        paste_burst_append_key(&mut state, KeyCode::Char(' '), KeyEventKind::Press, KeyModifiers::empty(), true);
        assert_eq!(paste_burst_live_document(&state), Some(("# ".into(), 2)));
    }

    #[test]
    fn burst_rewind_includes_first_rapid_char() {
        let mut state = PasteBurstState::default();
        paste_burst_begin_with_rewind(&mut state, "draft h", 7);
        assert_eq!(state.anchor_text, "draft ");
        assert_eq!(state.anchor_cursor, 6);
        assert_eq!(state.buffer, "h");
        paste_burst_append_key(&mut state, KeyCode::Char('i'), KeyEventKind::Press, KeyModifiers::empty(), true);
        assert_eq!(paste_burst_finish(&mut state), Some(("draft hi".into(), 8)));
    }
}
