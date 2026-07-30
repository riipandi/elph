//! GUI-style editing shortcuts (Option+arrow, Cmd+delete, Shift+Enter).

use iocraft::prelude::*;

use super::super::state::TextareaState;
use crate::text_editing::WireEditResult;
use crate::text_editing::apply_wire_edit_key;

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
    _input_width: u16,
    pending_esc: &mut bool,
) -> bool {
    // Use the logical buffer cursor — `layout_cursor` only maps display position for
    // the trailing empty continuation row and must not drive delete/word edits.
    let cursor = state.cursor;
    let Some(result) = apply_wire_edit_key(code, kind, modifiers, true, *pending_esc, &state.text, cursor) else {
        return false;
    };
    apply_wire_to_state(state, result, pending_esc);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use iocraft::prelude::{KeyCode, KeyEventKind, KeyModifiers};

    #[test]
    fn ctrl_backspace_at_line_end_deletes_word_not_trailing_newline_only() {
        let mut state = TextareaState::from_text("hello\n".into());
        state.cursor = "hello".len();
        let mut esc = false;
        assert!(apply_wire_edit(
            KeyCode::Backspace,
            KeyEventKind::Press,
            KeyModifiers::CONTROL,
            &mut state,
            40,
            &mut esc,
        ));
        assert_eq!(state.text, "\n");
        assert_eq!(state.cursor, 0);
    }

    #[test]
    fn cmd_backspace_at_line_end_deletes_word_in_one_press() {
        let mut state = TextareaState::from_text("hello world\n".into());
        state.cursor = "hello world".len();
        let mut esc = false;
        assert!(apply_wire_edit(
            KeyCode::Backspace,
            KeyEventKind::Press,
            KeyModifiers::SUPER,
            &mut state,
            40,
            &mut esc,
        ));
        assert_eq!(state.text, "hello \n");
        assert_eq!(state.cursor, "hello ".len());
    }

    #[test]
    fn ctrl_backspace_on_single_line_still_deletes_word() {
        let mut state = TextareaState::from_text("hello world".into());
        state.cursor = state.text.len();
        let mut esc = false;
        assert!(apply_wire_edit(
            KeyCode::Backspace,
            KeyEventKind::Press,
            KeyModifiers::CONTROL,
            &mut state,
            40,
            &mut esc,
        ));
        assert_eq!(state.text, "hello ");
        assert_eq!(state.cursor, 6);
    }
}
