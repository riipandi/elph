use crokey::KeyCombination;
use crossterm::event::KeyEvent;
use iocraft::prelude::{KeyCode, KeyModifiers};

/// Builds a normalized [`KeyCombination`] from iocraft key parts.
pub fn key_combination(code: KeyCode, modifiers: KeyModifiers) -> KeyCombination {
    KeyCombination::from(KeyEvent::new(code, modifiers))
}
