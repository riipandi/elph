//! Prompt vs transcript focus for the main shell.

use elph_tui::text_editing::is_transcript_scroll_key;
use iocraft::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ShellFocus {
    #[default]
    Prompt,
    Transcript,
    StatusDialog,
}

/// Plain letter, space, or `/` — refocus the prompt and seed the first keystroke.
pub fn prompt_focus_char(code: KeyCode, modifiers: KeyModifiers) -> Option<char> {
    if !modifiers.is_empty() {
        return None;
    }
    match code {
        KeyCode::Char(' ') => Some(' '),
        KeyCode::Char('/') => Some('/'),
        KeyCode::Char(c) if c.is_ascii_alphabetic() => Some(c),
        _ => None,
    }
}

/// Ctrl+C / Ctrl+D — still honored while a modal inline dialog is open.
pub fn shell_global_shortcut(modifiers: KeyModifiers, code: KeyCode) -> bool {
    modifiers.contains(KeyModifiers::CONTROL)
        && matches!(
            code,
            KeyCode::Char('c') | KeyCode::Char('C') | KeyCode::Char('d') | KeyCode::Char('D')
        )
}

/// Toggle native text selection (mouse capture off/on).
///
/// **Ctrl+S** is the primary chord (reliable in raw mode). **Ctrl+Shift+S** is also
/// accepted when the host delivers it. Callers must skip this while the scoped-models
/// editor is open — there Ctrl+S means save.
pub fn is_text_select_toggle_key(modifiers: KeyModifiers, code: KeyCode) -> bool {
    if !modifiers.contains(KeyModifiers::CONTROL) {
        return false;
    }
    if modifiers.intersects(KeyModifiers::ALT | KeyModifiers::META) {
        return false;
    }
    matches!(code, KeyCode::Char('s') | KeyCode::Char('S'))
}

/// Keys that scroll the transcript when it has focus (plain arrows and Shift+arrows).
pub fn transcript_nav_key(code: KeyCode, kind: KeyEventKind, modifiers: KeyModifiers) -> bool {
    if kind == KeyEventKind::Release {
        return false;
    }
    if is_transcript_scroll_key(code, kind, modifiers) {
        return true;
    }
    !modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::META)
        && matches!(
            code,
            KeyCode::Up | KeyCode::Down | KeyCode::PageUp | KeyCode::PageDown | KeyCode::Home | KeyCode::End
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_focus_char_accepts_alphabet_space_and_slash() {
        assert_eq!(prompt_focus_char(KeyCode::Char('h'), KeyModifiers::empty()), Some('h'));
        assert_eq!(prompt_focus_char(KeyCode::Char('Z'), KeyModifiers::empty()), Some('Z'));
        assert_eq!(prompt_focus_char(KeyCode::Char(' '), KeyModifiers::empty()), Some(' '));
        assert_eq!(prompt_focus_char(KeyCode::Char('/'), KeyModifiers::empty()), Some('/'));
        assert_eq!(prompt_focus_char(KeyCode::Char('1'), KeyModifiers::empty()), None);
        assert_eq!(prompt_focus_char(KeyCode::Char('a'), KeyModifiers::CONTROL), None);
    }

    #[test]
    fn shell_global_shortcut_matches_ctrl_c_and_ctrl_d() {
        assert!(shell_global_shortcut(KeyModifiers::CONTROL, KeyCode::Char('c')));
        assert!(shell_global_shortcut(KeyModifiers::CONTROL, KeyCode::Char('D')));
        assert!(!shell_global_shortcut(KeyModifiers::empty(), KeyCode::Char('c')));
        assert!(!shell_global_shortcut(KeyModifiers::CONTROL, KeyCode::Char('q')));
    }

    #[test]
    fn text_select_toggle_accepts_ctrl_s_with_or_without_shift() {
        assert!(is_text_select_toggle_key(KeyModifiers::CONTROL, KeyCode::Char('s')));
        assert!(is_text_select_toggle_key(KeyModifiers::CONTROL, KeyCode::Char('S')));
        assert!(is_text_select_toggle_key(
            KeyModifiers::CONTROL | KeyModifiers::SHIFT,
            KeyCode::Char('s')
        ));
        assert!(!is_text_select_toggle_key(KeyModifiers::empty(), KeyCode::Char('s')));
        assert!(!is_text_select_toggle_key(
            KeyModifiers::CONTROL | KeyModifiers::ALT,
            KeyCode::Char('s')
        ));
        assert!(!is_text_select_toggle_key(KeyModifiers::CONTROL, KeyCode::Char('t')));
    }

    #[test]
    fn transcript_nav_includes_shift_arrows_and_page_keys() {
        assert!(transcript_nav_key(KeyCode::Up, KeyEventKind::Press, KeyModifiers::SHIFT));
        assert!(transcript_nav_key(KeyCode::PageUp, KeyEventKind::Press, KeyModifiers::empty()));
        assert!(!transcript_nav_key(
            KeyCode::Char('a'),
            KeyEventKind::Press,
            KeyModifiers::empty()
        ));
    }
}
