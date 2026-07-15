//! GUI-style text editing helpers for [`TextInput`] wrappers.
//!
//! Cursor offsets follow iocraft: **byte indices** into UTF-8 strings.

use iocraft::prelude::*;

/// Editing action triggered by platform-style keyboard shortcuts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextEditAction {
    WordLeft,
    WordRight,
    DeleteWordBackward,
    DeleteWordForward,
    DeleteToLineStart,
    DeleteToLineEnd,
    InsertNewline,
}

/// Returns true for characters treated as part of a word (GUI-style).
pub fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Byte offset of the previous word boundary (macOS Option+← / Linux Ctrl+←).
pub fn prev_word_offset(text: &str, cursor: usize) -> usize {
    let mut i = cursor.min(text.len());
    if i == 0 {
        return 0;
    }

    while i > 0 {
        let ch = text[..i].chars().last().unwrap();
        if is_word_char(ch) {
            break;
        }
        i -= ch.len_utf8();
    }

    while i > 0 {
        let ch = text[..i].chars().last().unwrap();
        if !is_word_char(ch) {
            break;
        }
        i -= ch.len_utf8();
    }

    i
}

/// Byte offset of the next word boundary (macOS Option+→ / Linux Ctrl+→).
pub fn next_word_offset(text: &str, cursor: usize) -> usize {
    let mut i = cursor.min(text.len());
    let len = text.len();

    while i < len {
        let ch = text[i..].chars().next().unwrap();
        if !is_word_char(ch) {
            break;
        }
        i += ch.len_utf8();
    }

    while i < len {
        let ch = text[i..].chars().next().unwrap();
        if is_word_char(ch) {
            break;
        }
        i += ch.len_utf8();
    }

    i
}

/// Start of the line containing `cursor` (byte offset).
pub fn line_start_offset(text: &str, cursor: usize) -> usize {
    text[..cursor.min(text.len())].rfind('\n').map(|i| i + 1).unwrap_or(0)
}

/// End of the line containing `cursor` (byte offset, before `\n` or EOF).
pub fn line_end_offset(text: &str, cursor: usize) -> usize {
    let cursor = cursor.min(text.len());
    text[cursor..].find('\n').map(|i| cursor + i).unwrap_or(text.len())
}

pub fn delete_word_backward(text: &str, cursor: usize) -> (String, usize) {
    let start = prev_word_offset(text, cursor);
    if start == cursor {
        return (text.to_string(), cursor);
    }
    let mut out = text.to_string();
    out.drain(start..cursor);
    (out, start)
}

pub fn delete_word_forward(text: &str, cursor: usize) -> (String, usize) {
    let end = next_word_offset(text, cursor);
    if end == cursor {
        return (text.to_string(), cursor);
    }
    let mut out = text.to_string();
    out.drain(cursor..end);
    (out, cursor)
}

pub fn delete_to_line_start(text: &str, cursor: usize) -> (String, usize) {
    let start = line_start_offset(text, cursor);
    if start == cursor {
        return (text.to_string(), cursor);
    }
    let mut out = text.to_string();
    out.drain(start..cursor);
    (out, start)
}

pub fn delete_to_line_end(text: &str, cursor: usize) -> (String, usize) {
    let end = line_end_offset(text, cursor);
    if end == cursor {
        return (text.to_string(), cursor);
    }
    let mut out = text.to_string();
    out.drain(cursor..end);
    (out, cursor)
}

pub fn insert_newline_at_cursor(text: &str, cursor: usize) -> (String, usize) {
    let cursor = cursor.min(text.len());
    let mut out = text.to_string();
    out.insert(cursor, '\n');
    (out, cursor + '\n'.len_utf8())
}

/// Map a key event to a [`TextEditAction`], if it is an enhanced editing shortcut.
///
/// Only matches shortcuts that iocraft [`TextInput`] does not handle itself
/// (see `CONTROL` / `ALT` / `SUPER` branches in upstream `text_input.rs`).
///
/// Set `after_esc` when the previous key was a lone `Esc` (macOS Option+arrow often
/// arrives as `Esc` then `Left`/`Right`, or `Esc`+`b`/`f` emacs word motion).
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

    // macOS terminals often map Cmd+Backspace/Delete to readline Ctrl+U / Ctrl+K (0x15 / 0x0b).
    if matches!(code, KeyCode::Char('u') | KeyCode::Char('U')) && ctrl_only {
        return Some(TextEditAction::DeleteToLineStart);
    }
    if matches!(code, KeyCode::Char('k') | KeyCode::Char('K')) && ctrl_only {
        return Some(TextEditAction::DeleteToLineEnd);
    }

    // macOS/iTerm: Option+←/→ often encode as Alt+b / Alt+f (emacs), not Alt+arrow.
    if matches!(code, KeyCode::Char('b') | KeyCode::Char('B')) && modifiers.intersects(word_mod) {
        return Some(TextEditAction::WordLeft);
    }
    if matches!(code, KeyCode::Char('f') | KeyCode::Char('F')) && modifiers.intersects(word_mod) {
        return Some(TextEditAction::WordRight);
    }

    // Terminal.app split sequence from `\x1b\x1b[D` / `\x1b\x1b[C`.
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
        // Prefer line delete when Super is present (CSI u) before word-mod Backspace.
        KeyCode::Backspace if super_only => Some(TextEditAction::DeleteToLineStart),
        KeyCode::Backspace if modifiers.intersects(word_mod) => Some(TextEditAction::DeleteWordBackward),
        KeyCode::Delete if super_only => Some(TextEditAction::DeleteToLineEnd),
        KeyCode::Delete if modifiers.intersects(word_mod) => Some(TextEditAction::DeleteWordForward),
        KeyCode::Enter if multiline && modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) => {
            Some(TextEditAction::InsertNewline)
        }
        _ => None,
    }
}

/// Apply an editing action at `cursor` (byte offset).
pub fn apply_action(action: TextEditAction, text: &str, cursor: usize) -> (String, usize) {
    match action {
        TextEditAction::WordLeft => (text.to_string(), prev_word_offset(text, cursor)),
        TextEditAction::WordRight => (text.to_string(), next_word_offset(text, cursor)),
        TextEditAction::DeleteWordBackward => delete_word_backward(text, cursor),
        TextEditAction::DeleteWordForward => delete_word_forward(text, cursor),
        TextEditAction::DeleteToLineStart => delete_to_line_start(text, cursor),
        TextEditAction::DeleteToLineEnd => delete_to_line_end(text, cursor),
        TextEditAction::InsertNewline => insert_newline_at_cursor(text, cursor),
    }
}

/// Wire GUI-style shortcuts into a [`TextInput`] backed by `value` and `input_handle`.
pub fn wire_editing_shortcuts(
    hooks: &mut Hooks,
    has_focus: bool,
    multiline: bool,
    mut value: State<String>,
    input_handle: Ref<TextInputHandle>,
) {
    let pending_esc = hooks.use_ref(|| false);

    hooks.use_terminal_events({
        let mut input_handle = input_handle;
        let mut pending_esc = pending_esc;
        move |event| {
            if !has_focus {
                return;
            }
            let TerminalEvent::Key(KeyEvent {
                code, kind, modifiers, ..
            }) = event
            else {
                return;
            };
            if kind == KeyEventKind::Release {
                return;
            }

            let action = if pending_esc.get() {
                pending_esc.set(false);
                match_key_to_action(code, modifiers, multiline, true)
                    .or_else(|| match_key_to_action(code, modifiers, multiline, false))
            } else if code == KeyCode::Esc && modifiers.is_empty() {
                pending_esc.set(true);
                return;
            } else {
                match_key_to_action(code, modifiers, multiline, false)
            };

            let Some(action) = action else {
                return;
            };

            let cursor = input_handle.read().cursor_offset();
            let text = value.read().clone();
            let (new_text, new_cursor) = apply_action(action, &text, cursor);
            if new_text != text {
                value.set(new_text);
            }
            input_handle.write().set_cursor_offset(new_cursor);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prev_word_from_end_of_second_word() {
        assert_eq!(prev_word_offset("hello world", 11), 6);
    }

    #[test]
    fn prev_word_from_middle_of_word() {
        assert_eq!(prev_word_offset("hello world", 8), 6);
    }

    #[test]
    fn prev_word_from_after_space() {
        assert_eq!(prev_word_offset("hello world", 5), 0);
    }

    #[test]
    fn next_word_from_start() {
        assert_eq!(next_word_offset("hello world", 0), 6);
    }

    #[test]
    fn delete_word_backward_removes_previous_word() {
        let (text, cursor) = delete_word_backward("hello world", 11);
        assert_eq!(text, "hello ");
        assert_eq!(cursor, 6);
    }

    #[test]
    fn delete_to_line_start_on_second_line() {
        let text = "line one\nline two";
        let cursor = text.len();
        let (out, pos) = delete_to_line_start(text, cursor);
        assert_eq!(out, "line one\n");
        assert_eq!(pos, 9);
    }

    #[test]
    fn match_ctrl_backspace() {
        let action = match_key_to_action(KeyCode::Backspace, KeyModifiers::CONTROL, false, false);
        assert_eq!(action, Some(TextEditAction::DeleteWordBackward));
    }

    #[test]
    fn match_cmd_backspace() {
        let action = match_key_to_action(KeyCode::Backspace, KeyModifiers::SUPER, false, false);
        assert_eq!(action, Some(TextEditAction::DeleteToLineStart));
    }

    #[test]
    fn match_macos_cmd_backspace_via_ctrl_u() {
        let action = match_key_to_action(KeyCode::Char('u'), KeyModifiers::CONTROL, false, false);
        assert_eq!(action, Some(TextEditAction::DeleteToLineStart));
    }

    #[test]
    fn match_macos_cmd_delete_via_ctrl_k() {
        let action = match_key_to_action(KeyCode::Char('k'), KeyModifiers::CONTROL, false, false);
        assert_eq!(action, Some(TextEditAction::DeleteToLineEnd));
    }

    #[test]
    fn match_alt_b_word_left() {
        let action = match_key_to_action(KeyCode::Char('b'), KeyModifiers::ALT, false, false);
        assert_eq!(action, Some(TextEditAction::WordLeft));
    }

    #[test]
    fn match_alt_f_word_right() {
        let action = match_key_to_action(KeyCode::Char('f'), KeyModifiers::ALT, false, false);
        assert_eq!(action, Some(TextEditAction::WordRight));
    }

    #[test]
    fn match_esc_then_left_word_left() {
        let action = match_key_to_action(KeyCode::Left, KeyModifiers::empty(), false, true);
        assert_eq!(action, Some(TextEditAction::WordLeft));
    }

    #[test]
    fn utf8_word_boundaries() {
        assert_eq!(prev_word_offset("café résumé", "café résumé".len()), 6);
    }
}
