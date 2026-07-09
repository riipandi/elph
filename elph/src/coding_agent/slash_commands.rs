//! Built-in slash command registry and dispatch.

use elph_tui::SlashCommand;

#[derive(Debug, Clone)]
pub struct BuiltinSlashCommand {
    pub name: &'static str,
    pub description: &'static str,
}

pub fn builtin_slash_commands() -> Vec<BuiltinSlashCommand> {
    vec![
        BuiltinSlashCommand {
            name: "settings",
            description: "Open settings menu",
        },
        BuiltinSlashCommand {
            name: "model",
            description: "Select model",
        },
        BuiltinSlashCommand {
            name: "export",
            description: "Export session (JSONL)",
        },
        BuiltinSlashCommand {
            name: "import",
            description: "Import session JSONL",
        },
        BuiltinSlashCommand {
            name: "copy",
            description: "Copy last agent message",
        },
        BuiltinSlashCommand {
            name: "name",
            description: "Set session display name",
        },
        BuiltinSlashCommand {
            name: "session",
            description: "Show session info",
        },
        BuiltinSlashCommand {
            name: "changelog",
            description: "Show changelog",
        },
        BuiltinSlashCommand {
            name: "hotkeys",
            description: "Show keyboard shortcuts",
        },
        BuiltinSlashCommand {
            name: "fork",
            description: "Fork from a message",
        },
        BuiltinSlashCommand {
            name: "clone",
            description: "Clone current session",
        },
        BuiltinSlashCommand {
            name: "tree",
            description: "Navigate session tree",
        },
        BuiltinSlashCommand {
            name: "trust",
            description: "Save project trust decision",
        },
        BuiltinSlashCommand {
            name: "login",
            description: "Configure provider auth",
        },
        BuiltinSlashCommand {
            name: "logout",
            description: "Remove provider auth",
        },
        BuiltinSlashCommand {
            name: "new",
            description: "Start a new session",
        },
        BuiltinSlashCommand {
            name: "compact",
            description: "Compact conversation history",
        },
        BuiltinSlashCommand {
            name: "resume",
            description: "Resume a different session",
        },
        BuiltinSlashCommand {
            name: "reload",
            description: "Reload resources",
        },
        BuiltinSlashCommand {
            name: "quit",
            description: "Quit Elph",
        },
        BuiltinSlashCommand {
            name: "help",
            description: "List commands",
        },
        BuiltinSlashCommand {
            name: "exit",
            description: "Quit Elph",
        },
    ]
}

pub fn slash_commands_for_palette() -> Vec<SlashCommand> {
    builtin_slash_commands()
        .into_iter()
        .map(|cmd| SlashCommand::new(cmd.name, cmd.description))
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlashDispatch {
    Quit,
    Compact,
    NewSession,
    ShowSession,
    OpenModelSelector,
    OpenSessionSelector,
    OpenSettings,
    OpenTree,
    OpenLogin,
    Reload,
    Message(String),
    NotImplemented(String),
}

pub fn dispatch_slash_command(input: &str) -> Option<SlashDispatch> {
    let trimmed = input.trim();
    if !trimmed.starts_with('/') {
        return None;
    }
    let body = trimmed.trim_start_matches('/').trim();
    let (name, _args) = body.split_once(' ').map_or((body, ""), |(n, a)| (n, a));
    let name = name.to_ascii_lowercase();
    Some(match name.as_str() {
        "quit" | "exit" => SlashDispatch::Quit,
        "compact" | "c" => SlashDispatch::Compact,
        "new" => SlashDispatch::NewSession,
        "session" => SlashDispatch::ShowSession,
        "model" => SlashDispatch::OpenModelSelector,
        "resume" => SlashDispatch::OpenSessionSelector,
        "settings" => SlashDispatch::OpenSettings,
        "tree" | "fork" => SlashDispatch::OpenTree,
        "login" => SlashDispatch::OpenLogin,
        "reload" => SlashDispatch::Reload,
        "help" => SlashDispatch::Message("Type / to see commands.".into()),
        "changelog" | "hotkeys" | "copy" | "name" | "export" | "import" | "clone" | "trust" | "logout" | "share" => {
            SlashDispatch::NotImplemented(format!("/{name}"))
        }
        _ => SlashDispatch::NotImplemented(format!("/{name}")),
    })
}
