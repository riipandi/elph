//! GUI-style editing shortcuts (Option+arrow, Cmd+delete, Shift+Enter).

use iocraft::prelude::*;

use super::super::state::TextareaState;
use crate::text_editing::{WireEditResult, apply_wire_edit_key};

fn apply_wire_to_state(state: &mut TextareaState, result: WireEditResult, pending_esc: &mut bool) {
    *pending_esc = result.pending_esc;
    if result.text != state.text {
        state.text = result.text;
    }
    state.cursor = result.cursor;
    state.vertical_col_preference = None;
}

/// Apply wire-editing shortcuts when the key matches.
pub(crate) fn apply_wire_edit(
    code: KeyCode,
    kind: KeyEventKind,
    modifiers: KeyModifiers,
    state: &mut TextareaState,
    input_width: u16,
    pending_esc: &mut bool,
) -> bool {
    let cursor = state.layout_cursor(input_width);
    let Some(result) = apply_wire_edit_key(code, kind, modifiers, true, *pending_esc, &state.text, cursor) else {
        return false;
    };
    apply_wire_to_state(state, result, pending_esc);
    true
}
