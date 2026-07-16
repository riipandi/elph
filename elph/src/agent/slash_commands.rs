//! Built-in slash command registry and dispatch.

use crate::types::SlashCommand;
use elph_agent::{ExtensionRegistry, PromptTemplate};

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
        BuiltinSlashCommand {
            name: "goal",
            description: "Manage session goals",
        },
    ]
}

pub fn slash_commands_for_palette(
    extensions: Option<&ExtensionRegistry>,
    prompt_templates: Option<&[PromptTemplate]>,
) -> Vec<SlashCommand> {
    let mut commands: Vec<SlashCommand> = builtin_slash_commands()
        .into_iter()
        .map(|cmd| SlashCommand::new(cmd.name, cmd.description))
        .collect();
    let builtin_names: std::collections::HashSet<String> = commands.iter().map(|cmd| cmd.name.clone()).collect();

    if let Some(registry) = extensions {
        for cmd in registry.commands() {
            if !builtin_names.contains(&cmd.name) {
                commands.push(SlashCommand::new(cmd.name, cmd.description));
            }
        }
    }
    if let Some(templates) = prompt_templates {
        for template in templates {
            if !builtin_names.contains(&template.name) {
                commands.push(SlashCommand::new(&template.name, &template.description));
            }
        }
    }
    commands.sort_by(|a, b| a.name.cmp(&b.name));
    commands
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OverlayCommand {
    Model { filter: String },
    Tree,
    Resume,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlashDispatch {
    Quit,
    Compact,
    Goal { args: String },
    Help,
    Reload,
    Extension { name: String, args: String },
    PromptTemplate { name: String, args: String },
    OverlayNeeded(OverlayCommand),
    Unimplemented(String),
}

pub fn slash_unimplemented_message(command: &str) -> String {
    let name = command.trim_start_matches('/').trim();
    format!("{name} not yet implemented")
}

pub fn format_help_message(
    extensions: Option<&ExtensionRegistry>,
    prompt_templates: Option<&[PromptTemplate]>,
) -> String {
    let commands = slash_commands_for_palette(extensions, prompt_templates);
    let mut lines = vec!["Slash commands:".to_string()];
    for cmd in commands {
        lines.push(format!("  /{} — {}", cmd.name, cmd.description));
    }
    lines.join("\n")
}

fn split_slash_body(body: &str) -> (String, String) {
    let (name, args) = body.split_once(' ').map_or((body, ""), |(n, a)| (n, a));
    (name.to_ascii_lowercase(), args.trim().to_string())
}

fn builtin_dispatch(name: &str, args: String) -> Option<SlashDispatch> {
    match name {
        "exit" | "quit" | "q" => Some(SlashDispatch::Quit),
        "compact" | "c" => Some(SlashDispatch::Compact),
        "goal" | "goals" => Some(SlashDispatch::Goal { args }),
        "help" | "h" | "?" => Some(SlashDispatch::Help),
        "reload" => Some(SlashDispatch::Reload),
        "model" => Some(SlashDispatch::OverlayNeeded(OverlayCommand::Model { filter: args })),
        "tree" => Some(SlashDispatch::OverlayNeeded(OverlayCommand::Tree)),
        "resume" => Some(SlashDispatch::OverlayNeeded(OverlayCommand::Resume)),
        "settings" | "export" | "import" | "copy" | "name" | "session" | "changelog" | "hotkeys" | "fork" | "clone"
        | "trust" | "provider" | "new" => Some(SlashDispatch::Unimplemented(format!("/{name}"))),
        _ => None,
    }
}

pub fn dispatch_slash_command(
    input: &str,
    extensions: Option<&ExtensionRegistry>,
    prompt_templates: Option<&[PromptTemplate]>,
) -> Option<SlashDispatch> {
    let trimmed = input.trim();
    if !trimmed.starts_with('/') {
        return None;
    }
    let body = trimmed.trim_start_matches('/').trim();
    if body.is_empty() {
        return None;
    }
    let (name, args) = split_slash_body(body);

    if let Some(dispatch) = builtin_dispatch(&name, args.clone()) {
        return Some(dispatch);
    }

    if let Some(registry) = extensions
        && registry.commands().iter().any(|cmd| cmd.name == name)
    {
        return Some(SlashDispatch::Extension { name, args });
    }

    if let Some(templates) = prompt_templates
        && templates.iter().any(|template| template.name == name)
    {
        return Some(SlashDispatch::PromptTemplate { name, args });
    }

    Some(SlashDispatch::Unimplemented(format!("/{name}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unimplemented_message_uses_command_name_without_slash() {
        assert_eq!(slash_unimplemented_message("/settings"), "settings not yet implemented");
    }

    #[test]
    fn provider_subcommands_are_unimplemented() {
        assert_eq!(
            dispatch_slash_command("/provider connect", None, None),
            Some(SlashDispatch::Unimplemented("/provider".into()))
        );
        assert_eq!(
            dispatch_slash_command("/provider connect anthropic", None, None),
            Some(SlashDispatch::Unimplemented("/provider".into()))
        );
    }

    #[test]
    fn wired_commands_dispatch() {
        assert_eq!(dispatch_slash_command("/exit", None, None), Some(SlashDispatch::Quit));
        assert_eq!(dispatch_slash_command("/compact", None, None), Some(SlashDispatch::Compact));
        assert_eq!(
            dispatch_slash_command("/goal pause", None, None),
            Some(SlashDispatch::Goal { args: "pause".into() })
        );
        assert_eq!(dispatch_slash_command("/help", None, None), Some(SlashDispatch::Help));
        assert_eq!(dispatch_slash_command("/reload", None, None), Some(SlashDispatch::Reload));
    }

    #[test]
    fn overlay_commands_dispatch() {
        assert_eq!(
            dispatch_slash_command("/model opus", None, None),
            Some(SlashDispatch::OverlayNeeded(OverlayCommand::Model { filter: "opus".into() }))
        );
        assert_eq!(
            dispatch_slash_command("/tree", None, None),
            Some(SlashDispatch::OverlayNeeded(OverlayCommand::Tree))
        );
    }

    #[test]
    fn template_dispatch_when_no_extension() {
        let templates = vec![PromptTemplate {
            name: "review".into(),
            description: "Review code".into(),
            content: "Review $@".into(),
        }];
        assert_eq!(
            dispatch_slash_command("/review main.rs", None, Some(&templates)),
            Some(SlashDispatch::PromptTemplate {
                name: "review".into(),
                args: "main.rs".into()
            })
        );
    }

    #[test]
    fn palette_lists_goal_and_provider() {
        let names: Vec<_> = builtin_slash_commands().into_iter().map(|cmd| cmd.name).collect();
        assert!(names.contains(&"goal"));
        assert!(names.contains(&"provider"));
    }

    #[test]
    fn palette_skips_template_names_that_match_builtins() {
        let templates = vec![PromptTemplate {
            name: "help".into(),
            description: "Custom help".into(),
            content: "Help me".into(),
        }];
        let names: Vec<_> = slash_commands_for_palette(None, Some(&templates))
            .into_iter()
            .map(|cmd| cmd.name)
            .collect();
        assert_eq!(names.iter().filter(|name| **name == "help").count(), 1);
    }
}
