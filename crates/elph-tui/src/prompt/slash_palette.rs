use crate::components::inline_line;
use crate::diff::SlashCommand;
use crate::diff::fuzzy_filter;
use crate::theme::Theme;
use slt::{Context, KeyCode, KeyModifiers};

/// Built-in slash commands for the Elph TUI palette.
pub fn elph_builtin_commands() -> Vec<SlashCommand> {
    vec![
        SlashCommand::new("help", "List all commands"),
        SlashCommand::new("model", "Open model selector"),
        SlashCommand::new("exit", "Quit"),
        SlashCommand::new("quit", "Quit"),
        SlashCommand::new("changelog", "Show version history"),
        SlashCommand::new("compact", "Compact conversation history"),
        SlashCommand::new("goal", "Manage session goals"),
        SlashCommand::new("settings", "Open settings"),
    ]
}

/// Built-in slash commands for the Owly documentation shell.
pub fn owly_builtin_commands() -> Vec<SlashCommand> {
    vec![
        SlashCommand::new("help", "List commands"),
        SlashCommand::new("init", "Initialize openwiki"),
        SlashCommand::new("update", "Refresh documentation"),
        SlashCommand::new("history", "List recent checkpoints"),
        SlashCommand::new("restore", "Restore checkpoint (# or id)"),
        SlashCommand::new("clear", "Reset thread"),
        SlashCommand::new("exit", "Quit"),
        SlashCommand::new("quit", "Quit"),
    ]
}

/// Selection state for the fuzzy slash palette above the input.
#[derive(Debug, Clone, Default)]
pub struct SlashPaletteState {
    pub selected: usize,
    filter_key: String,
}

/// Outcome of slash palette keyboard handling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlashPaletteAction {
    None,
    Complete(String),
    Run(String),
    MoveUp,
    MoveDown,
}

/// Returns true when the slash palette should be visible.
pub fn slash_palette_visible(input: &str) -> bool {
    let trimmed = input.trim_start();
    trimmed.starts_with('/') && !trimmed.contains(' ')
}

fn filtered_commands(commands: &[SlashCommand], input: &str) -> Vec<SlashCommand> {
    let query = input.trim_start_matches('/').trim_start();
    if query.is_empty() {
        return commands.to_vec();
    }
    fuzzy_filter(commands, query, |cmd| cmd.name.clone())
}

/// Handle palette navigation keys. Call before normal prompt submit handling.
pub fn handle_slash_palette_keys(
    ui: &mut Context,
    state: &mut SlashPaletteState,
    input: &str,
    commands: &[SlashCommand],
) -> SlashPaletteAction {
    if !slash_palette_visible(input) {
        state.selected = 0;
        state.filter_key.clear();
        return SlashPaletteAction::None;
    }

    let query = input.trim_start_matches('/').trim_start();
    if state.filter_key != query {
        state.filter_key = query.to_string();
        state.selected = 0;
    }

    let filtered = filtered_commands(commands, input);
    if filtered.is_empty() {
        return SlashPaletteAction::None;
    }
    state.selected = state.selected.min(filtered.len().saturating_sub(1));

    if ui.key_code(KeyCode::Up) {
        state.selected = state.selected.saturating_sub(1);
        return SlashPaletteAction::MoveUp;
    }
    if ui.key_code(KeyCode::Down) {
        if state.selected + 1 < filtered.len() {
            state.selected += 1;
        }
        return SlashPaletteAction::MoveDown;
    }
    if ui.key_code(KeyCode::Tab) || ui.key_code(KeyCode::Right) {
        let cmd = &filtered[state.selected];
        return SlashPaletteAction::Complete(format!("/{} ", cmd.name));
    }
    let mut enter = None;
    for (index, key) in ui.key_presses_when(true) {
        if key.code == KeyCode::Enter && !key.modifiers.contains(KeyModifiers::SHIFT) {
            enter = Some(index);
            break;
        }
    }
    if let Some(index) = enter {
        ui.consume_event(index);
        let cmd = &filtered[state.selected];
        return SlashPaletteAction::Run(format!("/{}", cmd.name));
    }

    SlashPaletteAction::None
}

/// Renders the fuzzy command list above the input.
pub fn render_slash_palette(
    ui: &mut Context,
    input: &str,
    commands: &[SlashCommand],
    state: &SlashPaletteState,
    theme: Theme,
) {
    if !slash_palette_visible(input) {
        return;
    }
    let filtered = filtered_commands(commands, input);
    if filtered.is_empty() {
        return;
    }

    let pad = ui.spacing().xs();
    let _ = ui
        .bordered(slt::Border::Rounded)
        .border_fg(theme.blue_col())
        .p(pad)
        .gap(0)
        .col(|ui| {
            for (i, cmd) in filtered.iter().enumerate() {
                let marker = if i == state.selected { "› " } else { "  " };
                inline_line(ui, |ui| {
                    let _ = ui.text(marker).fg(if i == state.selected {
                        theme.highlight()
                    } else {
                        theme.dim_text()
                    });
                    let _ = ui.text(format!("/{}", cmd.name)).fg(theme.bright_text());
                    let _ = ui.text(format!("  {}", cmd.description)).fg(theme.dim_text()).dim();
                });
            }
        });
}
