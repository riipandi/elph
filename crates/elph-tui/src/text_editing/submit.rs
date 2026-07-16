//! Enter-submit and paste-burst timing helpers.

use std::time::{Duration, Instant};

use iocraft::prelude::*;

/// Keys arriving faster than this are treated as raw paste (Enter = newline, not submit).
pub const PASTE_BURST_WINDOW: Duration = Duration::from_millis(100);

/// Minimum raw-key echo suppression after bracketed paste (some terminals replay paste bytes).
pub const PASTE_ECHO_GUARD_BASE: Duration = Duration::from_millis(200);

/// Additional echo suppression per pasted byte (long pastes replay longer).
pub const PASTE_ECHO_GUARD_PER_CHAR: Duration = Duration::from_micros(1_000);

/// Plain Enter must not submit for this long after a paste completes (trailing terminal echo).
pub const PASTE_SUBMIT_GUARD_WINDOW: Duration = Duration::from_millis(150);

/// Total echo-suppression window for a paste of `paste_byte_len` bytes.
pub fn paste_echo_guard_duration(paste_byte_len: usize) -> Duration {
    PASTE_ECHO_GUARD_BASE.saturating_add(PASTE_ECHO_GUARD_PER_CHAR.saturating_mul(paste_byte_len as u32))
}

/// Returns true while plain Enter after paste should be ignored (trailing terminal echo).
pub fn paste_submit_guarded(suppress_submit_until: Option<Instant>, now: Instant) -> bool {
    suppress_submit_until.is_some_and(|t| now < t)
}

/// Returns true when `now` is within [`PASTE_BURST_WINDOW`] of the previous key event.
pub fn key_event_in_paste_burst(last_key_at: Option<Instant>, now: Instant) -> bool {
    last_key_at.is_some_and(|t| now.duration_since(t) < PASTE_BURST_WINDOW)
}

/// Shift+↑/↓ scrolls the transcript; the editor must not also move the caret.
pub fn is_transcript_scroll_key(code: KeyCode, kind: KeyEventKind, modifiers: KeyModifiers) -> bool {
    kind != KeyEventKind::Release
        && modifiers.contains(KeyModifiers::SHIFT)
        && !modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::META)
        && matches!(code, KeyCode::Up | KeyCode::Down)
}

/// Tab / → / Enter complete the slash palette — the editor must not move the caret or submit.
pub fn is_slash_palette_capture_key(code: KeyCode, kind: KeyEventKind, modifiers: KeyModifiers) -> bool {
    kind != KeyEventKind::Release
        && modifiers.is_empty()
        && matches!(code, KeyCode::Tab | KeyCode::Right | KeyCode::Enter)
}

/// Arrow / Home / End keys must not open the raw-paste burst window or advance `last_key_at`.
pub fn is_cursor_navigation_key(code: KeyCode, kind: KeyEventKind, modifiers: KeyModifiers) -> bool {
    kind != KeyEventKind::Release
        && !modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::META)
        && !modifiers.contains(KeyModifiers::SHIFT)
        && matches!(
            code,
            KeyCode::Up | KeyCode::Down | KeyCode::Left | KeyCode::Right | KeyCode::Home | KeyCode::End
        )
}

/// Plain `Enter` for chat submit (not Shift+Enter newline).
pub fn is_plain_submit_enter(
    multiline: bool,
    submit_on_enter: bool,
    code: KeyCode,
    kind: KeyEventKind,
    modifiers: KeyModifiers,
) -> bool {
    multiline
        && submit_on_enter
        && kind != KeyEventKind::Release
        && code == KeyCode::Enter
        && !modifiers.contains(KeyModifiers::SHIFT)
        && !modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::META)
}

/// Plain `Enter` should submit rather than insert a ghost newline.
#[allow(clippy::too_many_arguments)]
pub fn should_submit_on_enter(
    multiline: bool,
    submit_on_enter: bool,
    code: KeyCode,
    kind: KeyEventKind,
    modifiers: KeyModifiers,
    raw_paste_burst_active: bool,
    suppress_submit_until: Option<Instant>,
    now: Instant,
) -> bool {
    is_plain_submit_enter(multiline, submit_on_enter, code, kind, modifiers)
        && !raw_paste_burst_active
        && !paste_submit_guarded(suppress_submit_until, now)
}
