use slt::{Context, KeyCode, KeyModifiers};

/// Returns true for Tab (cycle agent mode when the prompt is empty).
fn is_mode_cycle_key(code: &KeyCode, modifiers: KeyModifiers) -> bool {
    matches!(code, KeyCode::Tab) && modifiers == KeyModifiers::NONE
}

/// Returns true for Ctrl+Tab (always cycle agent mode).
fn is_mode_cycle_override_key(code: &KeyCode, modifiers: KeyModifiers) -> bool {
    matches!(code, KeyCode::Tab) && modifiers.contains(KeyModifiers::CONTROL)
}

/// Returns true when the key should cycle agent mode.
pub fn should_cycle_agent_mode(text: &str, code: &KeyCode, modifiers: KeyModifiers) -> bool {
    if is_mode_cycle_override_key(code, modifiers) {
        return true;
    }
    is_mode_cycle_key(code, modifiers) && text.is_empty()
}

/// Consume Tab / Ctrl+Tab when they should cycle agent mode.
pub fn consume_mode_cycle_key(ui: &mut Context, text: &str) -> bool {
    let mut target = None;
    for (index, key) in ui.key_presses_when(true) {
        if should_cycle_agent_mode(text, &key.code, key.modifiers) {
            target = Some(index);
            break;
        }
    }
    if let Some(index) = target {
        ui.consume_event(index);
        true
    } else {
        false
    }
}

/// Consume Esc when clearing a non-empty prompt.
pub fn consume_prompt_clear(ui: &mut Context) -> bool {
    let mut target = None;
    for (index, key) in ui.key_presses_when(true) {
        if key.code == KeyCode::Esc && key.modifiers == KeyModifiers::NONE {
            target = Some(index);
            break;
        }
    }
    if let Some(index) = target {
        ui.consume_event(index);
        true
    } else {
        false
    }
}

/// Enter key intent (Shift+Enter is left for newline insertion in the textarea).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnterAction {
    None,
    /// Plain Enter — submit when idle, queue when the agent is busy.
    Submit,
    /// Ctrl+Enter — steer / interrupt the in-flight response.
    Steer,
}

/// Consume Enter / Ctrl+Enter; Shift+Enter is not consumed here.
pub fn consume_enter_action(ui: &mut Context) -> EnterAction {
    let mut target = None;
    let mut action = EnterAction::None;
    for (index, key) in ui.key_presses_when(true) {
        if key.code != KeyCode::Enter || key.modifiers.contains(KeyModifiers::SHIFT) {
            continue;
        }
        target = Some(index);
        action = if key.modifiers.contains(KeyModifiers::CONTROL) {
            EnterAction::Steer
        } else {
            EnterAction::Submit
        };
        break;
    }
    if let Some(index) = target {
        ui.consume_event(index);
        action
    } else {
        EnterAction::None
    }
}

/// Consume plain Enter (no Ctrl/Shift).
#[allow(dead_code)]
pub fn consume_submit_enter(ui: &mut Context) -> bool {
    matches!(consume_enter_action(ui), EnterAction::Submit)
}

/// Returns true when submitted text is the Neovim-style quit command (`:q`).
pub fn is_quit_command(text: &str) -> bool {
    matches!(text.trim(), ":q" | ":q!")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_quit_command() {
        assert!(is_quit_command(":q"));
        assert!(!is_quit_command("hello"));
    }

    #[test]
    fn cycles_mode_on_empty_prompt_tab() {
        assert!(should_cycle_agent_mode("", &KeyCode::Tab, KeyModifiers::NONE));
        assert!(!should_cycle_agent_mode("hi", &KeyCode::Tab, KeyModifiers::NONE));
    }

    #[test]
    fn cycles_mode_on_ctrl_tab() {
        assert!(should_cycle_agent_mode("busy", &KeyCode::Tab, KeyModifiers::CONTROL));
    }
}
