use crate::diff::SlashCommand;

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
