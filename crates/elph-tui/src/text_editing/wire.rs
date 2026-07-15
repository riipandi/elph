//! Wire-editing key dispatch (Option+arrow, Cmd+delete, Shift+Enter).

use iocraft::prelude::*;

use super::actions::{TextEditAction, apply_action};

/// Result of handling one wire-editing key press.
#[derive(Debug, PartialEq, Eq)]
pub struct WireEditResult {
    pub text: String,
    pub cursor: usize,
    pub pending_esc: bool,
    pub cursor_only: bool,
}

/// Map a key event to a [`TextEditAction`], if it is an enhanced editing shortcut.
pub fn match_key_to_action(
    code: KeyCode,
    modifiers: KeyModifiers,
    multiline: bool,
    after_esc: bool,
) -> Option<TextEditAction> {
    let word_mod = KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::META;
    let super_only = modifiers.contains(KeyModifiers::SUPER) && !modifiers.intersects(word_mod | KeyModifiers::SHIFT);
    let ctrl_only = modifiers.contains(KeyModifiers::CONTROL)
        && !modifiers.intersects(KeyModifiers::ALT | KeyModifiers::SUPER | KeyModifiers::SHIFT | KeyModifiers::META);

    if multiline
        && matches!(code, KeyCode::Enter)
        && modifiers.contains(KeyModifiers::SHIFT)
        && !modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::META)
    {
        return Some(TextEditAction::InsertNewline);
    }
    if multiline && matches!(code, KeyCode::Char('j') | KeyCode::Char('J')) && ctrl_only {
        return Some(TextEditAction::InsertNewline);
    }

    if matches!(code, KeyCode::Char('u') | KeyCode::Char('U')) && ctrl_only {
        return Some(TextEditAction::DeleteToLineStart);
    }
    if matches!(code, KeyCode::Char('k') | KeyCode::Char('K')) && ctrl_only {
        return Some(TextEditAction::DeleteToLineEnd);
    }

    if matches!(code, KeyCode::Char('b') | KeyCode::Char('B')) && modifiers.intersects(word_mod) {
        return Some(TextEditAction::WordLeft);
    }
    if matches!(code, KeyCode::Char('f') | KeyCode::Char('F')) && modifiers.intersects(word_mod) {
        return Some(TextEditAction::WordRight);
    }

    if after_esc && modifiers.is_empty() {
        match code {
            KeyCode::Left => return Some(TextEditAction::WordLeft),
            KeyCode::Right => return Some(TextEditAction::WordRight),
            _ => {}
        }
    }

    match code {
        KeyCode::Left if modifiers.intersects(word_mod) => Some(TextEditAction::WordLeft),
        KeyCode::Right if modifiers.intersects(word_mod) => Some(TextEditAction::WordRight),
        KeyCode::Backspace if super_only => Some(TextEditAction::DeleteToLineStart),
        KeyCode::Backspace if modifiers.intersects(word_mod) => Some(TextEditAction::DeleteWordBackward),
        KeyCode::Delete if super_only => Some(TextEditAction::DeleteToLineEnd),
        KeyCode::Delete if modifiers.intersects(word_mod) => Some(TextEditAction::DeleteWordForward),
        _ => None,
    }
}

/// Apply GUI-style editing shortcuts to `text` at `cursor` (byte offset).
pub fn apply_wire_edit_key(
    code: KeyCode,
    kind: KeyEventKind,
    modifiers: KeyModifiers,
    multiline: bool,
    pending_esc: bool,
    text: &str,
    cursor: usize,
) -> Option<WireEditResult> {
    if kind == KeyEventKind::Release {
        return None;
    }

    let (action, next_pending_esc) = if pending_esc {
        (
            match_key_to_action(code, modifiers, multiline, true)
                .or_else(|| match_key_to_action(code, modifiers, multiline, false)),
            false,
        )
    } else if code == KeyCode::Esc && modifiers.is_empty() {
        return Some(WireEditResult {
            text: text.to_string(),
            cursor,
            pending_esc: true,
            cursor_only: false,
        });
    } else {
        (match_key_to_action(code, modifiers, multiline, false), false)
    };

    let action = action?;
    let cursor = cursor.min(text.len());
    let (new_text, new_cursor) = apply_action(action, text, cursor);
    let text_changed = new_text != text;
    if !text_changed && new_cursor == cursor {
        return None;
    }
    Some(WireEditResult {
        text: if text_changed { new_text } else { text.to_string() },
        cursor: new_cursor,
        pending_esc: next_pending_esc,
        cursor_only: !text_changed,
    })
}

/// Apply a [`WireEditResult`] to string + byte cursor.
pub fn wire_edit_apply_to_cursor(
    result: WireEditResult,
    value: &mut String,
    cursor: &mut usize,
    pending_esc: &mut bool,
) {
    *pending_esc = result.pending_esc;
    if result.cursor_only {
        *cursor = result.cursor;
    } else {
        *value = result.text;
        *cursor = result.cursor;
    }
}

/// Apply a [`WireEditResult`] to live [`TextInput`] handle state.
pub fn wire_edit_apply_result(
    result: WireEditResult,
    value: &mut String,
    input_handle: &mut TextInputHandle,
    pending_esc: &mut bool,
) {
    let mut cursor = input_handle.cursor_offset();
    wire_edit_apply_to_cursor(result, value, &mut cursor, pending_esc);
    input_handle.set_cursor_offset(cursor);
}

/// Handle one wire-editing key against [`TextInput`] state.
pub fn wire_edit_handle_key(
    code: KeyCode,
    kind: KeyEventKind,
    modifiers: KeyModifiers,
    multiline: bool,
    pending_esc: &mut bool,
    value: &mut String,
    input_handle: &mut TextInputHandle,
) -> bool {
    let cursor = input_handle.cursor_offset();
    let Some(result) = apply_wire_edit_key(code, kind, modifiers, multiline, *pending_esc, value, cursor) else {
        return false;
    };
    wire_edit_apply_result(result, value, input_handle, pending_esc);
    true
}
