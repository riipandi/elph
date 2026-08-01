//! Keyboard resolution for the prompt history palette.

use iocraft::prelude::{KeyCode, KeyModifiers};

use super::model::PromptHistorySnapshot;

/// Outcome applied by the shell when the history palette is open (or opening).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromptHistoryKeyAction {
    MoveSelection(usize),
    /// Insert the selected history text into the prompt (Tab / Enter).
    ApplyToPrompt {
        text: String,
    },
    Dismiss,
}

pub fn resolve_key_action(
    snapshot: &PromptHistorySnapshot,
    selected_index: usize,
    code: KeyCode,
    modifiers: KeyModifiers,
) -> Option<PromptHistoryKeyAction> {
    if !snapshot.should_render() {
        return None;
    }
    if !modifiers.is_empty() {
        return None;
    }
    let len = snapshot.entries.len();
    match code {
        KeyCode::Esc => Some(PromptHistoryKeyAction::Dismiss),
        KeyCode::Tab | KeyCode::Enter => {
            let index = selected_index.min(len.saturating_sub(1));
            snapshot
                .entries
                .get(index)
                .map(|entry| PromptHistoryKeyAction::ApplyToPrompt {
                    text: entry.text.clone(),
                })
        }
        KeyCode::Up => {
            if len == 0 {
                return None;
            }
            let next = if selected_index == 0 {
                len - 1
            } else {
                selected_index - 1
            };
            Some(PromptHistoryKeyAction::MoveSelection(next))
        }
        KeyCode::Down => {
            if len == 0 {
                return None;
            }
            let next = (selected_index + 1) % len;
            Some(PromptHistoryKeyAction::MoveSelection(next))
        }
        _ => None,
    }
}

/// Whether this key may open the history palette (plain Arrow Up).
pub fn is_open_key(code: KeyCode, modifiers: KeyModifiers) -> bool {
    modifiers.is_empty() && code == KeyCode::Up
}

/// Whether an Arrow Up press is spaced far enough from the previous one to
/// treat as a deliberate keypress (not a mouse-wheel burst).
///
/// Call with the gap measured **before** updating the last-up timestamp.
pub fn is_deliberate_arrow_up(since_last_up: std::time::Duration) -> bool {
    since_last_up >= std::time::Duration::from_millis(50)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::prompt_history::model::{PromptHistoryEntry, PromptHistorySnapshot};

    fn snap(entries: &[&str]) -> PromptHistorySnapshot {
        PromptHistorySnapshot {
            visible: true,
            total_count: entries.len(),
            list_height: entries.len().max(1) as u16,
            entries: entries
                .iter()
                .map(|t| PromptHistoryEntry { text: (*t).to_string() })
                .collect(),
        }
    }

    #[test]
    fn tab_and_enter_apply_selected() {
        let s = snap(&["first", "second"]);
        assert_eq!(
            resolve_key_action(&s, 0, KeyCode::Tab, KeyModifiers::NONE),
            Some(PromptHistoryKeyAction::ApplyToPrompt { text: "first".into() })
        );
        assert_eq!(
            resolve_key_action(&s, 1, KeyCode::Enter, KeyModifiers::NONE),
            Some(PromptHistoryKeyAction::ApplyToPrompt { text: "second".into() })
        );
    }

    #[test]
    fn up_down_wrap() {
        let s = snap(&["a", "b", "c"]);
        assert_eq!(
            resolve_key_action(&s, 0, KeyCode::Up, KeyModifiers::NONE),
            Some(PromptHistoryKeyAction::MoveSelection(2))
        );
        assert_eq!(
            resolve_key_action(&s, 2, KeyCode::Down, KeyModifiers::NONE),
            Some(PromptHistoryKeyAction::MoveSelection(0))
        );
    }

    #[test]
    fn escape_dismisses() {
        let s = snap(&["a"]);
        assert_eq!(
            resolve_key_action(&s, 0, KeyCode::Esc, KeyModifiers::NONE),
            Some(PromptHistoryKeyAction::Dismiss)
        );
    }

    #[test]
    fn deliberate_arrow_up_rejects_burst_gap() {
        assert!(!is_deliberate_arrow_up(std::time::Duration::from_millis(0)));
        assert!(!is_deliberate_arrow_up(std::time::Duration::from_millis(49)));
        assert!(is_deliberate_arrow_up(std::time::Duration::from_millis(50)));
        assert!(is_deliberate_arrow_up(std::time::Duration::from_secs(2)));
    }
}
