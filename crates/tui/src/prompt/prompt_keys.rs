use iocraft::prelude::*;

/// Returns true when the key should insert a newline via `PromptInput` (Shift+Enter, Ctrl+J, or Ctrl+X).
pub fn is_prompt_newline_key(code: KeyCode, modifiers: KeyModifiers) -> bool {
    match code {
        KeyCode::Enter if modifiers.contains(KeyModifiers::SHIFT) => true,
        KeyCode::Char('j') | KeyCode::Char('J') if modifiers.contains(KeyModifiers::CONTROL) => true,
        KeyCode::Char('x') | KeyCode::Char('X') if modifiers.contains(KeyModifiers::CONTROL) => true,
        _ => false,
    }
}

/// Returns true for any newline shortcut (including Shift+Enter).
pub fn is_newline_key(code: KeyCode, modifiers: KeyModifiers) -> bool {
    is_prompt_newline_key(code, modifiers)
}

/// Returns true when the key should submit the prompt (plain Enter).
pub fn is_submit_key(code: KeyCode, modifiers: KeyModifiers) -> bool {
    code == KeyCode::Enter && !modifiers.intersects(KeyModifiers::SHIFT | KeyModifiers::CONTROL | KeyModifiers::ALT)
}

/// Returns true for Ctrl+C (interrupt: clear prompt first, then exit).
pub fn is_interrupt_key(code: KeyCode, modifiers: KeyModifiers) -> bool {
    code == KeyCode::Char('c') && modifiers.contains(KeyModifiers::CONTROL)
}

/// Returns true when submitted text is the Neovim-style quit command (`:q`).
pub fn is_quit_command(text: &str) -> bool {
    text.trim() == ":q"
}

/// Command (⌘) modifier without Control or Alt.
pub fn is_command(modifiers: KeyModifiers) -> bool {
    modifiers.contains(KeyModifiers::SUPER) && !modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
}

/// Option (⌥) / Alt modifier without Control or Super.
pub fn is_option(modifiers: KeyModifiers) -> bool {
    modifiers.contains(KeyModifiers::ALT) && !modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::SUPER)
}

/// Control modifier without Super or Alt (emacs / Windows-style shortcuts).
pub fn is_control_only(modifiers: KeyModifiers) -> bool {
    modifiers.contains(KeyModifiers::CONTROL) && !modifiers.intersects(KeyModifiers::ALT | KeyModifiers::SUPER)
}

/// Line/word editing shortcuts handled by `PromptInput`.
pub fn edit_action(code: KeyCode, modifiers: KeyModifiers) -> Option<EditAction> {
    if is_command(modifiers) {
        match code {
            KeyCode::Backspace => Some(EditAction::DeleteToLineStart),
            KeyCode::Delete => Some(EditAction::DeleteToLineEnd),
            KeyCode::Char('a') | KeyCode::Char('A') => Some(EditAction::LineStart),
            KeyCode::Char('e') | KeyCode::Char('E') => Some(EditAction::LineEnd),
            KeyCode::Left | KeyCode::Home => Some(EditAction::LineStart),
            KeyCode::Right | KeyCode::End => Some(EditAction::LineEnd),
            _ => None,
        }
    } else if is_option(modifiers) {
        match code {
            KeyCode::Backspace => Some(EditAction::DeleteWordBackward),
            KeyCode::Delete => Some(EditAction::DeleteWordForward),
            // macOS terminals often emit Option+←/→ as Alt+b / Alt+f (meta key bindings).
            KeyCode::Char('b') | KeyCode::Char('B') => Some(EditAction::WordLeft),
            KeyCode::Char('f') | KeyCode::Char('F') => Some(EditAction::WordRight),
            KeyCode::Char('d') | KeyCode::Char('D') => Some(EditAction::DeleteWordForward),
            KeyCode::Left => Some(EditAction::WordLeft),
            KeyCode::Right => Some(EditAction::WordRight),
            _ => None,
        }
    } else if is_control_only(modifiers) {
        match code {
            KeyCode::Char('a') | KeyCode::Char('A') => Some(EditAction::LineStart),
            KeyCode::Char('e') | KeyCode::Char('E') => Some(EditAction::LineEnd),
            KeyCode::Char('w') | KeyCode::Char('W') => Some(EditAction::DeleteWordBackward),
            KeyCode::Char('u') | KeyCode::Char('U') => Some(EditAction::DeleteToLineStart),
            KeyCode::Char('k') | KeyCode::Char('K') => Some(EditAction::DeleteToLineEnd),
            KeyCode::Char('b') | KeyCode::Char('B') => Some(EditAction::CharLeft),
            KeyCode::Char('f') | KeyCode::Char('F') => Some(EditAction::CharRight),
            KeyCode::Char('h') | KeyCode::Char('H') => Some(EditAction::DeleteCharBackward),
            KeyCode::Char('d') | KeyCode::Char('D') => Some(EditAction::DeleteCharForward),
            KeyCode::Backspace => Some(EditAction::DeleteWordBackward),
            KeyCode::Delete => Some(EditAction::DeleteWordForward),
            KeyCode::Left => Some(EditAction::WordLeft),
            KeyCode::Right => Some(EditAction::WordRight),
            KeyCode::Home => Some(EditAction::LineStart),
            KeyCode::End => Some(EditAction::LineEnd),
            _ => None,
        }
    } else {
        None
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
    fn detects_quit_command() {
        assert!(is_quit_command(":q"));
        assert!(is_quit_command(" :q "));
        assert!(!is_quit_command(":Q"));
        assert!(!is_quit_command(":q!"));
        assert!(!is_quit_command("hello"));
    }
}
