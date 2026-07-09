//! Cross-terminal Ctrl / modifier key matching.
//!
//! Many terminals send Ctrl+letter as a control character (`\x03` for Ctrl+C)
//! without the `CONTROL` modifier flag. Helpers here accept both forms.

use slt::{Context, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

/// ASCII control character produced by Ctrl+`letter` (e.g. Ctrl+C → `\x03`).
pub fn ctrl_char_for(letter: char) -> Option<char> {
    let upper = letter.to_ascii_uppercase();
    if upper.is_ascii_alphabetic() {
        char::from_u32((upper as u8 & 0x1f) as u32)
    } else {
        None
    }
}

fn letter_matches(c: char, letter: char) -> bool {
    c == letter || c == letter.to_ascii_lowercase() || c == letter.to_ascii_uppercase()
}

/// Returns true when `key` is a press of Ctrl+`letter` (modifier or legacy byte).
pub fn matches_ctrl_key(key: &KeyEvent, letter: char) -> bool {
    if key.kind != KeyEventKind::Press {
        return false;
    }

    if matches!(key.code, KeyCode::Char(c) if letter_matches(c, letter))
        && key.modifiers.contains(KeyModifiers::CONTROL)
    {
        return true;
    }

    if let KeyCode::Char(c) = &key.code
        && let Some(ctrl) = ctrl_char_for(letter)
    {
        return *c == ctrl;
    }

    false
}

/// Peek: Ctrl+`letter` pressed and not yet consumed.
pub fn pressed_ctrl_char(ui: &Context, letter: char) -> bool {
    ui.key_presses_when(true).any(|(_, key)| matches_ctrl_key(key, letter))
}

/// Consume Ctrl+`letter` if present this frame.
pub fn consume_ctrl_char(ui: &mut Context, letter: char) -> bool {
    let index = ui
        .key_presses_when(true)
        .find_map(|(i, key)| matches_ctrl_key(key, letter).then_some(i));
    if let Some(i) = index {
        ui.consume_event(i);
        true
    } else {
        false
    }
}

/// Consume a key code with required modifiers (e.g. Tab + Shift).
pub fn consume_key_code_mod(ui: &mut Context, code: KeyCode, modifiers: KeyModifiers) -> bool {
    let index = ui
        .key_presses_when(true)
        .find_map(|(i, key)| (key.code == code && key.modifiers.contains(modifiers)).then_some(i));
    if let Some(i) = index {
        ui.consume_event(i);
        true
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slt::Event;

    fn press(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        match Event::key_mod(code, modifiers) {
            Event::Key(key) => key,
            _ => panic!("expected key"),
        }
    }

    #[test]
    fn ctrl_char_mapping() {
        assert_eq!(ctrl_char_for('c'), Some('\x03'));
        assert_eq!(ctrl_char_for('Q'), Some('\x11'));
    }

    #[test]
    fn matches_modifier_form() {
        let key = press(KeyCode::Char('t'), KeyModifiers::CONTROL);
        assert!(matches_ctrl_key(&key, 't'));
    }

    #[test]
    fn matches_legacy_control_byte() {
        let key = press(KeyCode::Char('\x14'), KeyModifiers::NONE);
        assert!(matches_ctrl_key(&key, 't'));
    }
}
