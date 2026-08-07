//! Keyboard resolution for the `@` file picker.

use iocraft::prelude::{KeyCode, KeyModifiers};

use super::model::{
    FilePickerSnapshot, active_mention_at_cursor, complete_mention, mention_cursor_for_picker, selected_completion_path,
};

/// Palette-specific key outcome for the shell to apply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FilePickerKeyAction {
    MoveSelection(usize),
    CompleteDraft {
        text: String,
        suppress_enter_newline: bool,
    },
    /// `Esc` with a non-empty filter — clear the query but keep the picker open.
    ClearFilter,
    /// `Esc` with an empty filter — close the picker and leave `@` in the draft.
    Dismiss,
    ToggleHiddenFiles,
}

pub fn resolve_key_action(
    draft: &str,
    cursor: usize,
    snapshot: &FilePickerSnapshot,
    selected_index: usize,
    code: KeyCode,
    modifiers: KeyModifiers,
) -> Option<FilePickerKeyAction> {
    if modifiers.is_empty() && matches!(code, KeyCode::Esc) {
        let cursor = mention_cursor_for_picker(draft, cursor);
        if let Some(mention) = active_mention_at_cursor(draft, cursor) {
            if mention.query.is_empty() {
                return Some(FilePickerKeyAction::Dismiss);
            }
            return Some(FilePickerKeyAction::ClearFilter);
        }
        return Some(FilePickerKeyAction::Dismiss);
    }

    if !snapshot.visible {
        return None;
    }

    if modifiers.contains(KeyModifiers::CONTROL) && matches!(code, KeyCode::Char('.')) {
        return Some(FilePickerKeyAction::ToggleHiddenFiles);
    }

    match (modifiers, code) {
        (_, KeyCode::Tab) | (_, KeyCode::Right) => complete_action(draft, cursor, snapshot, selected_index, false),
        (_, KeyCode::Enter) if modifiers.is_empty() => complete_action(draft, cursor, snapshot, selected_index, true),
        (_, KeyCode::Up) | (_, KeyCode::Down) if modifiers.is_empty() => {
            if snapshot.options.is_empty() {
                return None;
            }
            Some(FilePickerKeyAction::MoveSelection(index_after_key(
                selected_index,
                snapshot.options.len(),
                code,
                super::model::FAST_SCROLL_STEP,
            )))
        }
        _ => None,
    }
}

fn complete_action(
    draft: &str,
    cursor: usize,
    snapshot: &FilePickerSnapshot,
    selected_index: usize,
    suppress_enter: bool,
) -> Option<FilePickerKeyAction> {
    let cursor = super::model::mention_cursor_for_picker(draft, cursor);
    let mention = super::model::active_mention_at_cursor(draft, cursor)?;
    if snapshot.options.is_empty() {
        return None;
    }
    let clamped = selected_index.min(snapshot.options.len() - 1);
    let path = selected_completion_path(&snapshot.options, clamped)?;
    Some(FilePickerKeyAction::CompleteDraft {
        text: complete_mention(draft, &mention, &path),
        suppress_enter_newline: suppress_enter,
    })
}

fn index_after_key(current: usize, len: usize, code: KeyCode, step: usize) -> usize {
    if len == 0 {
        return 0;
    }
    let delta = if matches!(code, KeyCode::Up) { -1 } else { 1 };
    let fast = false;
    let change = if fast { step as isize * delta } else { delta };
    let len = len as isize;
    let next = (current as isize + change).rem_euclid(len);
    next as usize
}

#[cfg(test)]
mod tests {
    use super::super::model::FilePickerOption;
    use super::*;

    fn sample_snapshot() -> FilePickerSnapshot {
        FilePickerSnapshot {
            visible: true,
            query: "ma".into(),
            options: vec![FilePickerOption {
                path: "src/main.rs".into(),
                is_directory: false,
            }],
            list_height: 1,
            match_count: 1,
            file_count: 1,
            dir_count: 0,
        }
    }

    #[test]
    fn tab_completes_selected_path() {
        let draft = "fix @ma";
        let snapshot = sample_snapshot();
        let action = resolve_key_action(draft, draft.len(), &snapshot, 0, KeyCode::Tab, KeyModifiers::NONE).unwrap();
        assert_eq!(
            action,
            FilePickerKeyAction::CompleteDraft {
                text: "fix @src/main.rs ".into(),
                suppress_enter_newline: false,
            }
        );
    }

    #[test]
    fn enter_completes_without_submitting() {
        let draft = "fix @ma";
        let snapshot = sample_snapshot();
        let action = resolve_key_action(draft, draft.len(), &snapshot, 0, KeyCode::Enter, KeyModifiers::NONE).unwrap();
        assert_eq!(
            action,
            FilePickerKeyAction::CompleteDraft {
                text: "fix @src/main.rs ".into(),
                suppress_enter_newline: true,
            }
        );
    }

    #[test]
    fn tab_completes_with_stale_cursor_before_eof_mention() {
        let draft = "fix @main";
        let snapshot = FilePickerSnapshot {
            visible: true,
            query: "main".into(),
            options: vec![FilePickerOption {
                path: "src/main.rs".into(),
                is_directory: false,
            }],
            list_height: 1,
            match_count: 1,
            file_count: 1,
            dir_count: 0,
        };
        let action = resolve_key_action(draft, 4, &snapshot, 0, KeyCode::Tab, KeyModifiers::NONE).unwrap();
        assert_eq!(
            action,
            FilePickerKeyAction::CompleteDraft {
                text: "fix @src/main.rs ".into(),
                suppress_enter_newline: false,
            }
        );
    }

    #[test]
    fn tab_completes_when_selection_index_out_of_bounds_after_filter() {
        let draft = "fix @main";
        let snapshot = FilePickerSnapshot {
            visible: true,
            query: "main".into(),
            options: vec![
                FilePickerOption {
                    path: "src/main.rs".into(),
                    is_directory: false,
                },
                FilePickerOption {
                    path: "lib/main.rs".into(),
                    is_directory: false,
                },
            ],
            list_height: 2,
            match_count: 2,
            file_count: 2,
            dir_count: 0,
        };
        let action = resolve_key_action(draft, draft.len(), &snapshot, 5, KeyCode::Tab, KeyModifiers::NONE).unwrap();
        assert_eq!(
            action,
            FilePickerKeyAction::CompleteDraft {
                text: "fix @lib/main.rs ".into(),
                suppress_enter_newline: false,
            }
        );
    }

    #[test]
    fn esc_clears_nonempty_filter_before_dismiss() {
        let snapshot = sample_snapshot();
        let draft = "fix @ma";
        let action = resolve_key_action(draft, draft.len(), &snapshot, 0, KeyCode::Esc, KeyModifiers::NONE).unwrap();
        assert_eq!(action, FilePickerKeyAction::ClearFilter);
    }

    #[test]
    fn esc_dismisses_when_filter_empty() {
        let snapshot = sample_snapshot();
        let draft = "fix @";
        let action = resolve_key_action(draft, draft.len(), &snapshot, 0, KeyCode::Esc, KeyModifiers::NONE).unwrap();
        assert_eq!(action, FilePickerKeyAction::Dismiss);
    }

    #[test]
    fn esc_clears_filter_even_when_snapshot_hidden() {
        let snapshot = FilePickerSnapshot::hidden();
        let draft = "fix @ma";
        let action = resolve_key_action(draft, draft.len(), &snapshot, 0, KeyCode::Esc, KeyModifiers::NONE).unwrap();
        assert_eq!(action, FilePickerKeyAction::ClearFilter);
    }

    #[test]
    fn tab_completes_directory_with_trailing_slash() {
        let draft = "open @src";
        let snapshot = FilePickerSnapshot {
            visible: true,
            query: "src".into(),
            options: vec![FilePickerOption {
                path: "src/".into(),
                is_directory: true,
            }],
            list_height: 1,
            match_count: 1,
            file_count: 0,
            dir_count: 1,
        };
        let action = resolve_key_action(draft, draft.len(), &snapshot, 0, KeyCode::Tab, KeyModifiers::NONE).unwrap();
        assert_eq!(
            action,
            FilePickerKeyAction::CompleteDraft {
                text: "open @src/ ".into(),
                suppress_enter_newline: false,
            }
        );
    }

    #[test]
    fn ctrl_period_toggles_hidden_files() {
        let snapshot = sample_snapshot();
        let action = resolve_key_action("x @a", 3, &snapshot, 0, KeyCode::Char('.'), KeyModifiers::CONTROL).unwrap();
        assert_eq!(action, FilePickerKeyAction::ToggleHiddenFiles);
    }
}
