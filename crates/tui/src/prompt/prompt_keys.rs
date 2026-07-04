use crate::terminal::key_combination;
use crokey::key;
use iocraft::prelude::*;

/// Returns true when the key should insert a newline via `PromptInput` (Shift+Enter, Ctrl+J, or Ctrl+X).
pub fn is_prompt_newline_key(code: KeyCode, modifiers: KeyModifiers) -> bool {
    matches!(
        key_combination(code, modifiers),
        key!(shift - enter) | key!(ctrl - j) | key!(ctrl - x)
    )
}

/// Returns true for any newline shortcut (including Shift+Enter).
pub fn is_newline_key(code: KeyCode, modifiers: KeyModifiers) -> bool {
    is_prompt_newline_key(code, modifiers)
}

/// Returns true when the key should submit the prompt (plain Enter).
pub fn is_submit_key(code: KeyCode, modifiers: KeyModifiers) -> bool {
    matches!(key_combination(code, modifiers), key!(enter))
}

/// Returns true for Ctrl+C (interrupt: clear prompt first, then exit).
pub fn is_interrupt_key(code: KeyCode, modifiers: KeyModifiers) -> bool {
    matches!(key_combination(code, modifiers), key!(ctrl - c))
}

/// Returns true for Ctrl+Q (force quit).
pub fn is_force_quit_key(code: KeyCode, modifiers: KeyModifiers) -> bool {
    matches!(key_combination(code, modifiers), key!(ctrl - q))
}

/// Returns true for Tab (cycle agent mode).
pub fn is_mode_cycle_key(code: KeyCode, modifiers: KeyModifiers) -> bool {
    matches!(key_combination(code, modifiers), key!(tab))
}

/// Returns true for Escape (clear prompt when non-empty).
pub fn is_clear_key(code: KeyCode, modifiers: KeyModifiers) -> bool {
    code == KeyCode::Esc && modifiers.is_empty()
}

/// Returns true for Ctrl+T (toggle theme).
pub fn is_theme_toggle_key(code: KeyCode, modifiers: KeyModifiers) -> bool {
    matches!(key_combination(code, modifiers), key!(ctrl - t))
}

/// Returns true when submitted text is the Neovim-style quit command (`:q`).
pub fn is_quit_command(text: &str) -> bool {
    text.trim() == ":q"
}

/// Line/word editing shortcuts handled by `PromptInput`.
pub fn edit_action(code: KeyCode, modifiers: KeyModifiers) -> Option<EditAction> {
    match key_combination(code, modifiers) {
        key!(cmd - backspace) => Some(EditAction::DeleteToLineStart),
        key!(cmd - delete) => Some(EditAction::DeleteToLineEnd),
        key!(cmd - a) | key!(cmd - left) | key!(cmd - home) => Some(EditAction::LineStart),
        key!(cmd - e) | key!(cmd - right) | key!(cmd - end) => Some(EditAction::LineEnd),

        key!(alt - backspace) => Some(EditAction::DeleteWordBackward),
        key!(alt - delete) => Some(EditAction::DeleteWordForward),
        key!(alt - b) | key!(alt - left) => Some(EditAction::WordLeft),
        key!(alt - f) | key!(alt - d) | key!(alt - right) => Some(EditAction::WordRight),

        key!(ctrl - a) | key!(ctrl - home) => Some(EditAction::LineStart),
        key!(ctrl - e) | key!(ctrl - end) => Some(EditAction::LineEnd),
        key!(ctrl - w) => Some(EditAction::DeleteWordBackward),
        key!(ctrl - u) => Some(EditAction::DeleteToLineStart),
        key!(ctrl - k) => Some(EditAction::DeleteToLineEnd),
        key!(ctrl - b) => Some(EditAction::CharLeft),
        key!(ctrl - f) => Some(EditAction::CharRight),
        key!(ctrl - h) => Some(EditAction::DeleteCharBackward),
        key!(ctrl - d) => Some(EditAction::DeleteCharForward),
        key!(ctrl - backspace) => Some(EditAction::DeleteWordBackward),
        key!(ctrl - delete) => Some(EditAction::DeleteWordForward),
        key!(ctrl - left) => Some(EditAction::WordLeft),
        key!(ctrl - right) => Some(EditAction::WordRight),

        _ => None,
    }
}

/// Alias for [`edit_action`].
pub fn mac_edit_action(code: KeyCode, modifiers: KeyModifiers) -> Option<EditAction> {
    edit_action(code, modifiers)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditAction {
    DeleteToLineStart,
    DeleteToLineEnd,
    DeleteWordBackward,
    DeleteWordForward,
    DeleteCharBackward,
    DeleteCharForward,
    LineStart,
    LineEnd,
    WordLeft,
    WordRight,
    CharLeft,
    CharRight,
}

/// Alias for [`EditAction`].
pub type MacEditAction = EditAction;

impl EditAction {
    pub fn modifies_text(self) -> bool {
        matches!(
            self,
            EditAction::DeleteToLineStart
                | EditAction::DeleteToLineEnd
                | EditAction::DeleteWordBackward
                | EditAction::DeleteWordForward
                | EditAction::DeleteCharBackward
                | EditAction::DeleteCharForward
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_newline_shortcuts() {
        assert!(is_newline_key(KeyCode::Enter, KeyModifiers::SHIFT,));
        assert!(is_prompt_newline_key(KeyCode::Enter, KeyModifiers::SHIFT,));
        assert!(is_prompt_newline_key(KeyCode::Char('j'), KeyModifiers::CONTROL,));
    }

    #[test]
    fn detects_interrupt() {
        assert!(is_interrupt_key(KeyCode::Char('c'), KeyModifiers::CONTROL,));
        assert!(!is_interrupt_key(KeyCode::Char('c'), KeyModifiers::empty(),));
    }

    #[test]
    fn detects_command_shortcuts() {
        assert_eq!(
            edit_action(KeyCode::Backspace, KeyModifiers::SUPER),
            Some(EditAction::DeleteToLineStart)
        );
        assert_eq!(
            edit_action(KeyCode::Left, KeyModifiers::ALT),
            Some(EditAction::WordLeft)
        );
        assert_eq!(
            edit_action(KeyCode::Right, KeyModifiers::SUPER),
            Some(EditAction::LineEnd)
        );
        assert_eq!(
            edit_action(KeyCode::Delete, KeyModifiers::SUPER),
            Some(EditAction::DeleteToLineEnd)
        );
    }

    #[test]
    fn detects_option_shortcuts() {
        assert_eq!(
            edit_action(KeyCode::Backspace, KeyModifiers::ALT),
            Some(EditAction::DeleteWordBackward)
        );
        assert_eq!(
            edit_action(KeyCode::Delete, KeyModifiers::ALT),
            Some(EditAction::DeleteWordForward)
        );
        assert_eq!(
            edit_action(KeyCode::Right, KeyModifiers::ALT),
            Some(EditAction::WordRight)
        );
        assert_eq!(
            edit_action(KeyCode::Char('b'), KeyModifiers::ALT),
            Some(EditAction::WordLeft)
        );
        assert_eq!(
            edit_action(KeyCode::Char('f'), KeyModifiers::ALT),
            Some(EditAction::WordRight)
        );
    }

    #[test]
    fn detects_control_shortcuts() {
        assert_eq!(
            edit_action(KeyCode::Char('a'), KeyModifiers::CONTROL),
            Some(EditAction::LineStart)
        );
        assert_eq!(
            edit_action(KeyCode::Char('e'), KeyModifiers::CONTROL),
            Some(EditAction::LineEnd)
        );
        assert_eq!(
            edit_action(KeyCode::Char('w'), KeyModifiers::CONTROL),
            Some(EditAction::DeleteWordBackward)
        );
        assert_eq!(
            edit_action(KeyCode::Char('u'), KeyModifiers::CONTROL),
            Some(EditAction::DeleteToLineStart)
        );
        assert_eq!(
            edit_action(KeyCode::Char('k'), KeyModifiers::CONTROL),
            Some(EditAction::DeleteToLineEnd)
        );
        assert_eq!(
            edit_action(KeyCode::Left, KeyModifiers::CONTROL),
            Some(EditAction::WordLeft)
        );
        assert_eq!(
            edit_action(KeyCode::Char('b'), KeyModifiers::CONTROL),
            Some(EditAction::CharLeft)
        );
        assert_eq!(
            edit_action(KeyCode::Char('f'), KeyModifiers::CONTROL),
            Some(EditAction::CharRight)
        );
        assert_eq!(
            edit_action(KeyCode::Backspace, KeyModifiers::CONTROL),
            Some(EditAction::DeleteWordBackward)
        );
    }

    #[test]
    fn ignores_mixed_modifiers() {
        assert_eq!(
            edit_action(KeyCode::Left, KeyModifiers::CONTROL | KeyModifiers::ALT,),
            None
        );
        assert_eq!(
            edit_action(KeyCode::Left, KeyModifiers::CONTROL | KeyModifiers::SUPER,),
            None
        );
    }

    #[test]
    fn detects_submit() {
        assert!(is_submit_key(KeyCode::Enter, KeyModifiers::empty()));
        assert!(!is_submit_key(KeyCode::Enter, KeyModifiers::SHIFT,));
    }

    #[test]
    fn detects_clear() {
        assert!(is_clear_key(KeyCode::Esc, KeyModifiers::empty()));
        assert!(!is_clear_key(KeyCode::Esc, KeyModifiers::SHIFT));
    }

    #[test]
    fn detects_quit_command() {
        assert!(is_quit_command(":q"));
        assert!(is_quit_command(" :q "));
        assert!(!is_quit_command(":Q"));
        assert!(!is_quit_command(":q!"));
        assert!(!is_quit_command("hello"));
    }
}
