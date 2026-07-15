//! Plain Enter submit and ghost-newline suppression.

use std::time::Instant;

use iocraft::prelude::*;

use super::super::state::TextareaState;
use super::TextareaInputResult;
use crate::text_editing::{is_plain_submit_enter, paste_submit_guarded, should_submit_on_enter};

/// Enter-key dispatch context.
pub(crate) struct EnterKey<'a> {
    pub code: KeyCode,
    pub kind: KeyEventKind,
    pub modifiers: KeyModifiers,
    pub now: Instant,
    pub state: &'a TextareaState,
    pub submit_on_enter: bool,
    pub suppress_enter_newline: Option<Ref<bool>>,
    pub raw_paste_burst_active: bool,
    pub suppress_submit_until: Option<Instant>,
}

/// Enter-key submit path and post-submit newline echo suppression.
///
/// Returns `Some` when the key was fully handled; `None` to continue normal dispatch.
pub(crate) fn handle_enter_key(key: EnterKey<'_>) -> Option<TextareaInputResult> {
    if paste_submit_guarded(key.suppress_submit_until, key.now)
        && key.kind != KeyEventKind::Release
        && is_plain_submit_enter(true, key.submit_on_enter, key.code, key.kind, key.modifiers)
    {
        return Some(TextareaInputResult::Consumed);
    }

    if should_submit_on_enter(
        true,
        key.submit_on_enter,
        key.code,
        key.kind,
        key.modifiers,
        key.raw_paste_burst_active,
        key.suppress_submit_until,
        key.now,
    ) {
        let draft = key.state.text.clone();
        if !draft.trim().is_empty() {
            if let Some(mut suppress) = key.suppress_enter_newline {
                suppress.set(true);
            }
            return Some(TextareaInputResult::Submit(draft));
        }
        return Some(TextareaInputResult::Consumed);
    }

    // Consume one ghost Enter only immediately after submit (before the user types again).
    if key.suppress_enter_newline.is_some_and(|s| s.get())
        && key.code == KeyCode::Enter
        && key.kind != KeyEventKind::Release
    {
        if let Some(mut suppress) = key.suppress_enter_newline {
            suppress.set(false);
        }
        return Some(TextareaInputResult::Consumed);
    }

    None
}
