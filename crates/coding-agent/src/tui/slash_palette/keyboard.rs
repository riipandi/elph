//! Keyboard resolution for the slash command palette.

use iocraft::prelude::{KeyCode, KeyModifiers};

use super::model::SlashPalettePhase;
use super::model::SlashPaletteSnapshot;
use super::model::{
    complete_command, complete_slash_arg, palette_submit_slash_input, palette_visible, query_from_draft,
    selected_arg_value, selected_command_name,
};
use crate::agent::slash_arg_completions;
use crate::agent::slash_palette_submit_on_enter;
use crate::types::SlashCommand;

use super::model::parse_slash_draft;

fn overlay_or_complete_action(draft: &str, command_name: &str) -> SlashPaletteKeyAction {
    if slash_palette_submit_on_enter(command_name) {
        SlashPaletteKeyAction::SubmitCommand {
            slash_input: palette_submit_slash_input(draft, command_name),
        }
    } else {
        SlashPaletteKeyAction::CompleteDraft {
            text: complete_command(draft, command_name),
            suppress_enter_newline: false,
        }
    }
}

fn args_complete_action(
    draft: &str,
    command: &str,
    options: &[elph_tui::types::SelectOption],
    selected_index: usize,
) -> Option<SlashPaletteKeyAction> {
    let arg = selected_arg_value(options, selected_index)?;
    Some(SlashPaletteKeyAction::CompleteDraft {
        text: complete_slash_arg(draft, command, arg),
        suppress_enter_newline: false,
    })
}

fn enter_args_palette_action(
    draft: &str,
    command: &str,
    options: &[elph_tui::types::SelectOption],
    selected_index: usize,
) -> Option<SlashPaletteKeyAction> {
    if let Some(parts) = parse_slash_draft(draft) {
        let query = parts.args_query;
        if !query.is_empty() {
            let exact_known =
                slash_arg_completions(command).is_some_and(|entries| entries.iter().any(|entry| entry.value == query));
            let no_palette_prefix = options
                .iter()
                .all(|option| !option.name.to_ascii_lowercase().starts_with(&query));
            if exact_known || no_palette_prefix {
                return Some(SlashPaletteKeyAction::SubmitCommand {
                    slash_input: draft.trim().to_string(),
                });
            }
        }
    }

    let arg = selected_arg_value(options, selected_index)?;
    Some(SlashPaletteKeyAction::SubmitCommand {
        slash_input: complete_slash_arg(draft, command, arg).trim().to_string(),
    })
}

fn enter_palette_action(
    draft: &str,
    filtered_commands: &[SlashCommand],
    selected_index: usize,
) -> Option<SlashPaletteKeyAction> {
    if let Some(name) = selected_command_name(filtered_commands, selected_index) {
        return Some(if slash_palette_submit_on_enter(name) {
            SlashPaletteKeyAction::SubmitCommand {
                slash_input: palette_submit_slash_input(draft, name),
            }
        } else {
            SlashPaletteKeyAction::CompleteDraft {
                text: complete_command(draft, name),
                suppress_enter_newline: true,
            }
        });
    }

    let query = query_from_draft(draft)?;
    if slash_palette_submit_on_enter(&query) {
        return Some(SlashPaletteKeyAction::SubmitCommand {
            slash_input: palette_submit_slash_input(draft, &query),
        });
    }
    None
}

/// Palette-specific key outcome for the shell to apply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlashPaletteKeyAction {
    MoveSelection(usize),
    CompleteDraft {
        text: String,
        /// When true, the shell sets `suppress_enter_newline` so the editor does not submit.
        suppress_enter_newline: bool,
    },
    /// Close the palette and return to normal prompt input.
    Dismiss,
    /// Submit an overlay slash command (for example `/model`) from the palette.
    SubmitCommand {
        slash_input: String,
    },
}

pub fn resolve_key_action(
    draft: &str,
    filtered_commands: &[SlashCommand],
    selected_index: usize,
    code: KeyCode,
    modifiers: KeyModifiers,
) -> Option<SlashPaletteKeyAction> {
    if !palette_visible(draft) {
        return None;
    }

    match (modifiers, code) {
        (_, KeyCode::Esc) if modifiers.is_empty() => Some(SlashPaletteKeyAction::Dismiss),
        (_, KeyCode::Tab) | (_, KeyCode::Right) => {
            selected_command_name(filtered_commands, selected_index).map(|name| overlay_or_complete_action(draft, name))
        }
        (_, KeyCode::Enter) if modifiers.is_empty() => enter_palette_action(draft, filtered_commands, selected_index),
        (_, KeyCode::Up) | (_, KeyCode::Down) if modifiers.is_empty() => {
            if filtered_commands.is_empty() {
                return None;
            }
            Some(SlashPaletteKeyAction::MoveSelection(index_after_key(
                selected_index,
                filtered_commands.len(),
                code,
                modifiers,
                super::model::FAST_SCROLL_STEP,
            )))
        }
        _ => None,
    }
}

pub fn resolve_snapshot_key_action(
    draft: &str,
    snapshot: &SlashPaletteSnapshot,
    selected_index: usize,
    code: KeyCode,
    modifiers: KeyModifiers,
) -> Option<SlashPaletteKeyAction> {
    if !snapshot.should_render() {
        return None;
    }
    if let SlashPalettePhase::Args { command } = &snapshot.phase {
        if !palette_visible(draft) {
            return None;
        }
        return match (modifiers, code) {
            (_, KeyCode::Esc) if modifiers.is_empty() => Some(SlashPaletteKeyAction::Dismiss),
            (_, KeyCode::Tab) | (_, KeyCode::Right) => {
                args_complete_action(draft, command, &snapshot.options, selected_index)
            }
            (_, KeyCode::Enter) if modifiers.is_empty() => {
                enter_args_palette_action(draft, command, &snapshot.options, selected_index)
            }
            (_, KeyCode::Up) | (_, KeyCode::Down) if modifiers.is_empty() => {
                if snapshot.options.is_empty() {
                    return None;
                }
                Some(SlashPaletteKeyAction::MoveSelection(index_after_key(
                    selected_index,
                    snapshot.options.len(),
                    code,
                    modifiers,
                    super::model::FAST_SCROLL_STEP,
                )))
            }
            _ => None,
        };
    }
    resolve_key_action(draft, &snapshot.filtered_commands, selected_index, code, modifiers)
}

fn index_after_key(current: usize, len: usize, code: KeyCode, modifiers: KeyModifiers, step: usize) -> usize {
    if len == 0 {
        return 0;
    }
    let fast = modifiers.contains(KeyModifiers::SHIFT);
    let delta = if fast { step as isize } else { 1 };
    let change = match code {
        KeyCode::Up => Some(-delta),
        KeyCode::Down => Some(delta),
        _ => None,
    };
    let Some(change) = change else {
        return current;
    };
    let len = len as isize;
    let next = (current as isize + change).rem_euclid(len);
    next as usize
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::SlashCommand;

    fn sample_commands() -> Vec<SlashCommand> {
        vec![
            SlashCommand::new("compact", "Compact history"),
            SlashCommand::new("goal", "Manage goals"),
            SlashCommand::new("model", "Select model"),
        ]
    }

    #[test]
    fn tab_completes_selected_command() {
        let draft = "/go";
        let commands = sample_commands();
        let filtered = super::super::model::filter_commands(&commands, "go");
        let action = resolve_key_action(draft, &filtered, 0, KeyCode::Tab, KeyModifiers::NONE).unwrap();
        assert_eq!(
            action,
            SlashPaletteKeyAction::CompleteDraft {
                text: "/goal ".into(),
                suppress_enter_newline: false,
            }
        );
    }

    #[test]
    fn enter_completes_selected_command_without_submitting() {
        let draft = "/go";
        let commands = sample_commands();
        let filtered = super::super::model::filter_commands(&commands, "go");
        let action = resolve_key_action(draft, &filtered, 0, KeyCode::Enter, KeyModifiers::NONE).unwrap();
        assert_eq!(
            action,
            SlashPaletteKeyAction::CompleteDraft {
                text: "/goal ".into(),
                suppress_enter_newline: true,
            }
        );
    }

    #[test]
    fn tab_on_model_submits_overlay_command() {
        let draft = "/model";
        let commands = sample_commands();
        let filtered = super::super::model::filter_commands(&commands, "model");
        let index = filtered.iter().position(|cmd| cmd.name == "model").expect("model");
        let action = resolve_key_action(draft, &filtered, index, KeyCode::Tab, KeyModifiers::NONE).unwrap();
        assert_eq!(
            action,
            SlashPaletteKeyAction::SubmitCommand {
                slash_input: "/model".into(),
            }
        );
    }

    #[test]
    fn enter_on_model_submits_overlay_command() {
        let draft = "/model";
        let commands = sample_commands();
        let filtered = super::super::model::filter_commands(&commands, "model");
        let index = filtered.iter().position(|cmd| cmd.name == "model").expect("model");
        let action = resolve_key_action(draft, &filtered, index, KeyCode::Enter, KeyModifiers::NONE).unwrap();
        assert_eq!(
            action,
            SlashPaletteKeyAction::SubmitCommand {
                slash_input: "/model".into(),
            }
        );
    }

    #[test]
    fn enter_on_partial_model_submits_overlay_command() {
        let draft = "/mod";
        let commands = sample_commands();
        let filtered = super::super::model::filter_commands(&commands, "mod");
        let index = filtered.iter().position(|cmd| cmd.name == "model").expect("model");
        let action = resolve_key_action(draft, &filtered, index, KeyCode::Enter, KeyModifiers::NONE).unwrap();
        assert_eq!(
            action,
            SlashPaletteKeyAction::SubmitCommand {
                slash_input: "/model".into(),
            }
        );
    }

    #[test]
    fn down_moves_selection_within_bounds() {
        let commands = sample_commands();
        let action = resolve_key_action("/g", &commands, 0, KeyCode::Down, KeyModifiers::NONE).unwrap();
        assert_eq!(action, SlashPaletteKeyAction::MoveSelection(1));
    }

    #[test]
    fn down_from_last_item_wraps_to_first() {
        let commands = sample_commands();
        let last = commands.len() - 1;
        let action = resolve_key_action("/g", &commands, last, KeyCode::Down, KeyModifiers::NONE).unwrap();
        assert_eq!(action, SlashPaletteKeyAction::MoveSelection(0));
    }

    #[test]
    fn up_from_first_item_wraps_to_last() {
        let commands = sample_commands();
        let last = commands.len() - 1;
        let action = resolve_key_action("/g", &commands, 0, KeyCode::Up, KeyModifiers::NONE).unwrap();
        assert_eq!(action, SlashPaletteKeyAction::MoveSelection(last));
    }

    #[test]
    fn ignores_keys_when_palette_hidden() {
        assert!(resolve_key_action("hello", &sample_commands(), 0, KeyCode::Tab, KeyModifiers::NONE).is_none());
    }

    #[test]
    fn escape_dismisses_palette() {
        let action = resolve_key_action("/go", &sample_commands(), 0, KeyCode::Esc, KeyModifiers::NONE).unwrap();
        assert_eq!(action, SlashPaletteKeyAction::Dismiss);
    }

    #[test]
    fn letter_keys_are_not_intercepted() {
        assert!(resolve_key_action("/go", &sample_commands(), 0, KeyCode::Char('j'), KeyModifiers::NONE).is_none());
        assert!(resolve_key_action("/go", &sample_commands(), 0, KeyCode::Char('o'), KeyModifiers::NONE).is_none());
    }

    #[test]
    fn keys_ignored_when_command_has_no_arg_completions() {
        assert!(resolve_key_action("/help args", &sample_commands(), 0, KeyCode::Down, KeyModifiers::NONE).is_none());
    }

    #[test]
    fn args_phase_closes_when_arg_is_complete() {
        use super::super::model::{build_snapshot, palette_visible};

        let mut commands = sample_commands();
        commands.push(crate::types::SlashCommand::new("tools", "Show tools").with_args_hint("[list|table]"));
        assert!(!palette_visible("/tools list"));
        let snapshot = build_snapshot("/tools list", &commands, 40);
        assert!(!snapshot.should_render());
        assert!(resolve_snapshot_key_action("/tools list", &snapshot, 0, KeyCode::Enter, KeyModifiers::NONE).is_none());
    }

    #[test]
    fn args_phase_enter_completes_partial_arg_and_submits() {
        use super::super::model::build_snapshot;

        let mut commands = sample_commands();
        commands.push(
            crate::types::SlashCommand::new("confetti", "Confetti celebration").with_args_hint("[confetti|firework]"),
        );
        let snapshot = build_snapshot("/confetti c", &commands, 40);
        let action =
            resolve_snapshot_key_action("/confetti c", &snapshot, 0, KeyCode::Enter, KeyModifiers::NONE).unwrap();
        assert_eq!(
            action,
            SlashPaletteKeyAction::SubmitCommand {
                slash_input: "/confetti confetti".into(),
            }
        );
    }

    #[test]
    fn args_phase_tab_completes_confetti_format() {
        use super::super::model::build_snapshot;

        let mut commands = sample_commands();
        commands.push(
            crate::types::SlashCommand::new("confetti", "Confetti celebration").with_args_hint("[confetti|firework]"),
        );
        let snapshot = build_snapshot("/confetti c", &commands, 40);
        let action =
            resolve_snapshot_key_action("/confetti c", &snapshot, 0, KeyCode::Tab, KeyModifiers::NONE).unwrap();
        assert_eq!(
            action,
            SlashPaletteKeyAction::CompleteDraft {
                text: "/confetti confetti ".into(),
                suppress_enter_newline: false,
            }
        );
    }

    #[test]
    fn confetti_tab_completes_command_without_submitting() {
        use super::super::model::build_snapshot;
        use crate::types::SlashCommand;

        let commands = vec![
            SlashCommand::new("confetti", "Confetti celebration")
                .with_args_hint("[confetti|firework]")
                .with_hidden(true),
        ];
        // Command phase: Tab fills `/confetti ` so the args palette can open.
        let filtered = super::super::model::filter_commands(&commands, "conf");
        let action = resolve_key_action("/conf", &filtered, 0, KeyCode::Tab, KeyModifiers::NONE).unwrap();
        assert_eq!(
            action,
            SlashPaletteKeyAction::CompleteDraft {
                text: "/confetti ".into(),
                suppress_enter_newline: false,
            }
        );

        // Args phase: palette lists confetti | firework; Enter submits selected mode.
        let snapshot = build_snapshot("/confetti ", &commands, 40);
        assert!(matches!(snapshot.phase, super::super::model::SlashPalettePhase::Args { .. }));
        assert!(snapshot.options.iter().any(|opt| opt.name == "firework"));
        let action =
            resolve_snapshot_key_action("/confetti ", &snapshot, 1, KeyCode::Enter, KeyModifiers::NONE).unwrap();
        assert_eq!(
            action,
            SlashPaletteKeyAction::SubmitCommand {
                slash_input: "/confetti firework".into(),
            }
        );
    }
}
