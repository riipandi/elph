//! Built-in slash command registry and dispatch.

use elph_agent::ExtensionRegistry;
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
            name: "provider",
            description: "Manage providers (connect, disconnect)",
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

pub fn slash_commands_for_palette(extensions: Option<&ExtensionRegistry>) -> Vec<SlashCommand> {
    let mut commands: Vec<SlashCommand> = builtin_slash_commands()
        .into_iter()
        .map(|cmd| SlashCommand::new(cmd.name, cmd.description))
        .collect();
    if let Some(registry) = extensions {
        for cmd in registry.commands() {
            commands.push(SlashCommand::new(cmd.name, cmd.description));
        }
    }
    commands.sort_by(|a, b| a.name.cmp(&b.name));
    commands
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlashDispatch {
    Stub(String),
}

pub fn slash_stub_message(command: &str) -> String {
    format!("{command} not yet implemented!")
}

fn split_slash_body(body: &str) -> (String, String) {
    let (name, args) = body.split_once(' ').map_or((body, ""), |(n, a)| (n, a));
    (name.to_ascii_lowercase(), args.trim().to_string())
}

pub fn dispatch_slash_command(input: &str, _extensions: Option<&ExtensionRegistry>) -> Option<SlashDispatch> {
    let trimmed = input.trim();
    if !trimmed.starts_with('/') {
        return None;
    }
    let body = trimmed.trim_start_matches('/').trim();
    let (name, _args) = split_slash_body(body);
    Some(SlashDispatch::Stub(format!("/{name}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_uses_command_with_action_args() {
        let (name, args) = split_slash_body("provider connect anthropic");
        assert_eq!(name, "provider");
        assert_eq!(args, "connect anthropic");

        assert_eq!(
            dispatch_slash_command("/provider connect", None),
            Some(SlashDispatch::Stub("/provider".into()))
        );
        assert_eq!(
            dispatch_slash_command("/provider connect anthropic", None),
            Some(SlashDispatch::Stub("/provider".into()))
        );
        assert_eq!(
            dispatch_slash_command("/provider disconnect openai", None),
            Some(SlashDispatch::Stub("/provider".into()))
        );
    }

    #[test]
    fn stub_message_format() {
        assert_eq!(slash_stub_message("/model"), "/model not yet implemented!");
    }

    #[test]
    fn palette_lists_provider_commands() {
        let names: Vec<_> = builtin_slash_commands().into_iter().map(|cmd| cmd.name).collect();
        assert!(names.contains(&"provider"));
        assert!(!names.contains(&"provider connect"));
        assert!(!names.contains(&"provider disconnect"));
        assert!(!names.contains(&"login"));
        assert!(!names.contains(&"logout"));
    }
}
