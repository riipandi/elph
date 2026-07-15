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
///
/// Word motion does not cross line boundaries.
pub fn prev_word_offset(text: &str, cursor: usize) -> usize {
    let mut i = cursor.min(text.len());
    let line_start = line_start_offset(text, cursor);
    if i <= line_start {
        return line_start;
    }

    while i > line_start {
        let ch = text[..i].chars().last().unwrap();
        if is_word_char(ch) {
            break;
        }
        i -= ch.len_utf8();
    }

    while i > line_start {
        let ch = text[..i].chars().last().unwrap();
        if !is_word_char(ch) {
            break;
        }
        i -= ch.len_utf8();
    }

    i
}

/// Byte offset of the next word boundary (macOS Option+→ / Linux Ctrl+→).
///
/// Word motion does not cross line boundaries.
pub fn next_word_offset(text: &str, cursor: usize) -> usize {
    let mut i = cursor.min(text.len());
    let line_end = line_end_offset(text, cursor);

    while i < line_end {
        let ch = text[i..].chars().next().unwrap();
        if !is_word_char(ch) {
            break;
        }
        i += ch.len_utf8();
    }

    while i < line_end {
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

fn line_is_blank(line: &str) -> bool {
    line.chars().all(|c| c.is_whitespace())
}

/// When the cursor is at column 0, delete the preceding newline (join with previous line).
fn delete_preceding_newline_if_at_line_start(text: &str, cursor: usize) -> Option<(String, usize)> {
    let cursor = cursor.min(text.len());
    let start = line_start_offset(text, cursor);
    if start == cursor && start > 0 && text.as_bytes().get(start - 1) == Some(&b'\n') {
        let mut out = text.to_string();
        out.remove(start - 1);
        Some((out, start - 1))
    } else {
        None
    }
}

/// Cmd+Backspace at column 0 on a blank line: remove all contiguous blank lines above.
fn delete_blank_lines_backward(text: &str, cursor: usize) -> Option<(String, usize)> {
    let cursor = cursor.min(text.len());
    let line_start = line_start_offset(text, cursor);
    if line_start != cursor || line_start == 0 {
        return None;
    }

    let line_end = line_end_offset(text, cursor);
    let current_line = &text[line_start..line_end];
    if !line_is_blank(current_line) {
        return delete_preceding_newline_if_at_line_start(text, cursor);
    }

    let mut delete_from = line_start;
    let mut scan_line_start = line_start;

    loop {
        if scan_line_start == 0 {
            break;
        }
        let prev_newline = scan_line_start - 1;
        if text.as_bytes().get(prev_newline) != Some(&b'\n') {
            break;
        }
        let prev_line_start = line_start_offset(text, prev_newline);
        let prev_line = &text[prev_line_start..prev_newline];
        if line_is_blank(prev_line) {
            delete_from = prev_line_start;
            scan_line_start = prev_line_start;
        } else {
            delete_from = prev_newline;
            break;
        }
    }

    if delete_from == line_start {
        return delete_preceding_newline_if_at_line_start(text, cursor);
    }

    let mut out = text.to_string();
    out.drain(delete_from..line_start);
    Some((out, delete_from))
}

pub fn delete_word_backward(text: &str, cursor: usize) -> (String, usize) {
    let start = prev_word_offset(text, cursor);
    if start == cursor {
        return delete_preceding_newline_if_at_line_start(text, cursor).unwrap_or_else(|| (text.to_string(), cursor));
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
    let cursor = cursor.min(text.len());
    let start = line_start_offset(text, cursor);
    if start == cursor {
        return delete_blank_lines_backward(text, cursor)
            .or_else(|| delete_preceding_newline_if_at_line_start(text, cursor))
            .unwrap_or_else(|| (text.to_string(), cursor));
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

/// Newline insert via [`wire_editing_shortcuts`] (Shift+Enter / Ctrl+J).
///
/// Appending at EOF places the cursor on the empty continuation row (`text.len()`), not on the
/// `\n` byte — avoids the one-frame "empty row below" layout glitch when the handle lags.
pub fn wire_insert_newline(text: &str, cursor: usize) -> (String, usize) {
    let cursor = cursor.min(text.len());
    let (new_text, mut new_cursor) = insert_newline_at_cursor(text, cursor);
    if new_text.ends_with('\n') && cursor >= text.len() {
        new_cursor = new_text.len();
    }
    (new_text, new_cursor)
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

    // Chat editor newline: Shift+Enter and Ctrl+J. Plain Enter is submit (handled by the app shell).
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
    cursor_snapshot: Ref<usize>,
    pending_newline: Option<Ref<bool>>,
) {
    let pending_esc = hooks.use_ref(|| false);

    hooks.use_terminal_events({
        let mut input_handle = input_handle;
        let mut cursor_snapshot = cursor_snapshot;
        let mut pending_esc = pending_esc;
        let pending_newline = pending_newline;
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

            let cursor = cursor_snapshot.get().min(value.read().len());
            let text = value.read().clone();
            let (new_text, mut new_cursor) = apply_action(action, &text, cursor);
            let shift_enter = matches!(code, KeyCode::Enter) && modifiers.contains(KeyModifiers::SHIFT);
            if action == TextEditAction::InsertNewline {
                if new_text.ends_with('\n') && cursor >= text.len() {
                    new_cursor = new_text.len();
                }
                if let Some(mut pending) = pending_newline {
                    pending.set(true);
                }
            }
            let text_changed = new_text != text;
            let changed = text_changed || new_cursor != cursor;
            if text_changed {
                value.set(new_text);
            }
            if changed {
                cursor_snapshot.set(new_cursor);
                if !text_changed {
                    input_handle.write().set_cursor_offset(new_cursor);
                }
            }
            if action == TextEditAction::InsertNewline
                && !shift_enter
                && let Some(mut pending) = pending_newline
            {
                pending.set(false);
            }
        }
    });
}
