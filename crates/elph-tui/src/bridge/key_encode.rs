//! Converts SLT key events into raw terminal sequences for diff components.

use slt::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

/// Encodes an SLT key event as raw terminal input understood by diff-TUI components.
pub fn key_event_to_terminal_data(event: &KeyEvent) -> Option<String> {
    if event.kind == KeyEventKind::Release {
        return None;
    }

    let mods = event.modifiers;

    if mods.contains(KeyModifiers::CONTROL) && matches!(event.code, KeyCode::Char('c')) {
        return Some("\x03".to_string());
    }

    match &event.code {
        KeyCode::Up => Some("\x1b[A".to_string()),
        KeyCode::Down => Some("\x1b[B".to_string()),
        KeyCode::Left => Some("\x1b[D".to_string()),
        KeyCode::Right => Some("\x1b[C".to_string()),
        KeyCode::Enter => Some("\r".to_string()),
        KeyCode::Esc => Some("\x1b".to_string()),
        KeyCode::Tab => Some("\t".to_string()),
        KeyCode::Backspace => Some("\x7f".to_string()),
        KeyCode::Delete => Some("\x1b[3~".to_string()),
        KeyCode::Home => Some("\x1b[H".to_string()),
        KeyCode::End => Some("\x1b[F".to_string()),
        KeyCode::Char(ch) => {
            if mods.contains(KeyModifiers::CONTROL) && ch.is_ascii_alphabetic() {
                let byte = (*ch as u8) & 0x1f;
                return Some(String::from(char::from(byte)));
            }
            Some(ch.to_string())
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        let slt::Event::Key(event) = slt::Event::key_mod(code, modifiers) else {
            panic!("expected key event");
        };
        event
    }

    #[test]
    fn encodes_arrow_keys() {
        let up = press(KeyCode::Up, KeyModifiers::NONE);
        assert_eq!(key_event_to_terminal_data(&up).as_deref(), Some("\x1b[A"));
    }

    #[test]
    fn encodes_ctrl_c() {
        let ev = press(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert_eq!(key_event_to_terminal_data(&ev).as_deref(), Some("\x03"));
    }

    #[test]
    fn ignores_release_events() {
        let slt::Event::Key(mut ev) = slt::Event::key(KeyCode::Enter) else {
            panic!("expected key event");
        };
        ev.kind = KeyEventKind::Release;
        assert!(key_event_to_terminal_data(&ev).is_none());
    }
}
