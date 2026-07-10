//! Prompt textarea keybindings.
//!
//! All cursor navigation and delete-word bindings are handled here *before*
//! [`Context::textarea`] runs, using [`Context::key_presses_when`] so they work
//! reliably even when chat scroll or focus order would otherwise swallow arrow
//! keys. Plain arrows, Home/End, and Emacs/word-style shortcuts are included.

mod delete;
mod grapheme;
mod movement;

use slt::{Context, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, TextareaState};

use delete::{delete_to_line_end, delete_to_line_start, delete_word_backward, delete_word_forward, insert_newline};
use grapheme::{
    has_word_modifier, is_delete_word_modifier, is_newline_key, is_plain_navigation, is_super_only,
    is_word_nav_modifier,
};
use movement::{
    move_char_left, move_char_right, move_line_down, move_line_end, move_line_start, move_line_up, move_word_left,
    move_word_right,
};

/// Apply one prompt key binding. Returns `true` when the event was handled.
pub fn apply_textarea_key(state: &mut TextareaState, key: &KeyEvent) -> bool {
    if is_newline_key(key) {
        insert_newline(state);
        return true;
    }

    if key.kind != KeyEventKind::Press {
        return false;
    }

    match key.code {
        KeyCode::Left if is_super_only(key.modifiers) => {
            move_line_start(state);
            true
        }
        KeyCode::Right if is_super_only(key.modifiers) => {
            move_line_end(state);
            true
        }
        KeyCode::Left if is_word_nav_modifier(key.modifiers) => {
            move_word_left(state);
            true
        }
        KeyCode::Right if is_word_nav_modifier(key.modifiers) => {
            move_word_right(state);
            true
        }
        KeyCode::Left if is_plain_navigation(key.modifiers) => {
            move_char_left(state);
            true
        }
        KeyCode::Right if is_plain_navigation(key.modifiers) => {
            move_char_right(state);
            true
        }
        KeyCode::Up if is_plain_navigation(key.modifiers) => {
            move_line_up(state);
            true
        }
        KeyCode::Down if is_plain_navigation(key.modifiers) => {
            move_line_down(state);
            true
        }
        KeyCode::Home if is_plain_navigation(key.modifiers) || has_word_modifier(key.modifiers) => {
            move_line_start(state);
            true
        }
        KeyCode::End if is_plain_navigation(key.modifiers) || has_word_modifier(key.modifiers) => {
            move_line_end(state);
            true
        }
        KeyCode::Char('b') if is_word_nav_modifier(key.modifiers) => {
            move_word_left(state);
            true
        }
        KeyCode::Char('f') if is_word_nav_modifier(key.modifiers) => {
            move_word_right(state);
            true
        }
        KeyCode::Backspace if is_delete_word_modifier(key.modifiers) => {
            if is_super_only(key.modifiers) || key.modifiers.contains(KeyModifiers::CONTROL) {
                delete_to_line_start(state);
            } else {
                delete_word_backward(state);
            }
            true
        }
        KeyCode::Delete if is_delete_word_modifier(key.modifiers) => {
            if is_super_only(key.modifiers) {
                delete_to_line_end(state);
            } else {
                delete_word_forward(state);
            }
            true
        }
        KeyCode::Char('w') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            delete_word_backward(state);
            true
        }
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            delete_to_line_start(state);
            true
        }
        _ => false,
    }
}

/// Consume prompt navigation/delete keys before [`Context::textarea`] runs.
pub fn consume_prompt_textarea_keys(ui: &mut Context, state: &mut TextareaState, active: bool) {
    let mut consumed = Vec::new();
    for (index, key) in ui.key_presses_when(active) {
        if apply_textarea_key(state, key) {
            consumed.push(index);
        }
    }
    for index in consumed {
        ui.consume_event(index);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slt::{Event, KeyEvent};

    fn press(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        match Event::key_mod(code, modifiers) {
            Event::Key(key) => key,
            _ => panic!("expected key event"),
        }
    }

    fn textarea_with(text: &str) -> TextareaState {
        let mut state = TextareaState::new();
        state.set_value(text);
        state.cursor_col = grapheme::grapheme_count(text);
        state
    }

    #[test]
    fn plain_left_moves_cursor() {
        let mut state = textarea_with("hello");
        state.cursor_col = grapheme::grapheme_count("hello");
        assert!(apply_textarea_key(
            &mut state,
            &press(KeyCode::Left, KeyModifiers::NONE)
        ));
        assert_eq!(state.cursor_col, grapheme::grapheme_count("hell"));
    }

    #[test]
    fn alt_left_jumps_to_previous_word() {
        let mut state = textarea_with("hello world");
        state.cursor_col = grapheme::grapheme_count("hello world");
        assert!(apply_textarea_key(&mut state, &press(KeyCode::Left, KeyModifiers::ALT)));
        assert_eq!(state.cursor_col, grapheme::grapheme_count("hello "));
    }

    #[test]
    fn alt_b_jumps_to_previous_word() {
        let mut state = textarea_with("hello world");
        state.cursor_col = grapheme::grapheme_count("hello world");
        assert!(apply_textarea_key(
            &mut state,
            &press(KeyCode::Char('b'), KeyModifiers::ALT)
        ));
        assert_eq!(state.cursor_col, grapheme::grapheme_count("hello "));
    }

    #[test]
    fn alt_backspace_deletes_previous_word() {
        let mut state = textarea_with("hello world");
        state.cursor_col = grapheme::grapheme_count("hello world");
        assert!(apply_textarea_key(
            &mut state,
            &press(KeyCode::Backspace, KeyModifiers::ALT)
        ));
        assert_eq!(state.value(), "hello ");
    }

    #[test]
    fn super_left_moves_to_line_start() {
        let mut state = textarea_with("hello world");
        state.cursor_col = grapheme::grapheme_count("hello world");
        assert!(apply_textarea_key(
            &mut state,
            &press(KeyCode::Left, KeyModifiers::SUPER)
        ));
        assert_eq!(state.cursor_col, 0);
    }

    #[test]
    fn shift_enter_inserts_newline() {
        let mut state = textarea_with("hello");
        assert!(apply_textarea_key(
            &mut state,
            &press(KeyCode::Enter, KeyModifiers::SHIFT)
        ));
        assert_eq!(state.lines.len(), 2);
        assert_eq!(state.lines[0], "hello");
    }

    #[test]
    fn ctrl_j_inserts_newline() {
        let mut state = textarea_with("hello");
        assert!(apply_textarea_key(
            &mut state,
            &press(KeyCode::Char('\x0a'), KeyModifiers::NONE)
        ));
        assert_eq!(state.lines.len(), 2);
    }

    #[test]
    fn ctrl_u_deletes_to_line_start() {
        let mut state = textarea_with("hello world");
        state.cursor_col = grapheme::grapheme_count("hello ");
        assert!(apply_textarea_key(
            &mut state,
            &press(KeyCode::Char('u'), KeyModifiers::CONTROL)
        ));
        assert_eq!(state.value(), "world");
    }
}
