//! Built-in slash command registry and dispatch.

use crate::agent::{parse_skill_slash, skill_slash_name, truncate_skill_palette_description};
use crate::types::SlashCommand;
use elph_agent::{ExtensionRegistry, PromptTemplate, Skill};

#[derive(Debug, Clone)]
pub struct BuiltinSlashCommand {
    pub name: &'static str,
    pub description: &'static str,
    pub args_hint: Option<&'static str>,
    pub hidden: bool,
}

fn builtin(name: &'static str, description: &'static str) -> BuiltinSlashCommand {
    BuiltinSlashCommand {
        name,
        description,
        args_hint: None,
        hidden: false,
    }
}

fn builtin_with_args(name: &'static str, description: &'static str, args_hint: &'static str) -> BuiltinSlashCommand {
    BuiltinSlashCommand {
        name,
        description,
        args_hint: Some(args_hint),
        hidden: false,
    }
}

fn hidden_builtin_with_args(
    name: &'static str,
    description: &'static str,
    args_hint: &'static str,
) -> BuiltinSlashCommand {
    BuiltinSlashCommand {
        name,
        description,
        args_hint: Some(args_hint),
        hidden: true,
    }
}

pub fn builtin_slash_commands() -> Vec<BuiltinSlashCommand> {
    vec![
        builtin("settings", "Open settings menu"),
        builtin_with_args("model", "Select model", "[filter]"),
        builtin("export", "Export session (JSONL)"),
        builtin("import", "Import session JSONL"),
        builtin("copy", "Copy last agent message"),
        builtin("name", "Set session display name"),
        builtin("session", "Show session info"),
        builtin("changelog", "Show changelog"),
        builtin("hotkeys", "Show keyboard shortcuts"),
        builtin("fork", "Fork from a message"),
        builtin("clone", "Clone current session"),
        builtin("tree", "Navigate session tree"),
        builtin("trust", "Save project trust decision"),
        builtin("provider", "Manage providers (connect, disconnect)"),
        builtin("new", "Start a new session"),
        builtin("compact", "Compact conversation history"),
        builtin("resume", "Resume a different session"),
        builtin("reload", "Reload resources"),
        builtin("quit", "Quit Elph"),
        builtin("help", "List commands"),
        builtin_with_args("tools", "Show active tools", "[json|list|table]"),
        builtin("system-prompt", "Show compiled system prompt"),
        builtin("exit", "Quit Elph"),
        builtin_with_args("goal", "Manage session goals", "<subcommand>"),
        hidden_builtin_with_args("confetti", "Confetti celebration", "[confetti|firework]"),
    ]
}

pub fn slash_commands_for_palette(
    extensions: Option<&ExtensionRegistry>,
    prompt_templates: Option<&[PromptTemplate]>,
    skills: Option<&[Skill]>,
) -> Vec<SlashCommand> {
    let mut commands: Vec<SlashCommand> = builtin_slash_commands()
        .into_iter()
        .filter(|cmd| !cmd.hidden)
        .map(|cmd| {
            let mut entry = SlashCommand::new(cmd.name, cmd.description);
            if let Some(hint) = cmd.args_hint {
                entry = entry.with_args_hint(hint);
            }
            entry
        })
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
    if let Some(skills) = skills {
        for skill in skills {
            let name = skill_slash_name(&skill.name);
            if !builtin_names.contains(&name) {
                commands.push(SlashCommand::new(name, truncate_skill_palette_description(&skill.description)));
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
    Tools { args: String },
    SystemPrompt,
    Confetti { args: String },
    Reload,
    Extension { name: String, args: String },
    PromptTemplate { name: String, args: String },
    Skill { name: String, args: String },
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
    skills: Option<&[Skill]>,
) -> String {
    let commands = slash_commands_for_palette(extensions, prompt_templates, skills);
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

/// One selectable argument value for slash-command arg autocompletion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlashArgCompletion {
    pub value: &'static str,
    pub description: &'static str,
}

const TOOLS_ARG_COMPLETIONS: &[SlashArgCompletion] = &[
    SlashArgCompletion {
        value: "table",
        description: "Markdown table (default)",
    },
    SlashArgCompletion {
        value: "json",
        description: "Pretty-printed JSON",
    },
    SlashArgCompletion {
        value: "list",
        description: "Grouped bullet list",
    },
];

const CONFETTI_ARG_COMPLETIONS: &[SlashArgCompletion] = &[
    SlashArgCompletion {
        value: "confetti",
        description: "Rain confetti from the top",
    },
    SlashArgCompletion {
        value: "firework",
        description: "Fireworks from the bottom",
    },
];

const GOAL_ARG_COMPLETIONS: &[SlashArgCompletion] = &[
    SlashArgCompletion {
        value: "status",
        description: "Show current goal",
    },
    SlashArgCompletion {
        value: "pause",
        description: "Pause active goal",
    },
    SlashArgCompletion {
        value: "resume",
        description: "Resume paused goal",
    },
    SlashArgCompletion {
        value: "cancel",
        description: "Clear active goal",
    },
    SlashArgCompletion {
        value: "replace",
        description: "Replace goal objective",
    },
    SlashArgCompletion {
        value: "next",
        description: "Queue next goal (unimplemented)",
    },
];

/// Static arg suggestions for built-in slash commands (palette args phase).
pub fn slash_arg_completions(command_name: &str) -> Option<&'static [SlashArgCompletion]> {
    match command_name {
        "tools" => Some(TOOLS_ARG_COMPLETIONS),
        "goal" | "goals" => Some(GOAL_ARG_COMPLETIONS),
        "confetti" | "conffety" | "confetty" => Some(CONFETTI_ARG_COMPLETIONS),
        _ => None,
    }
}

/// Parse `/confetti` mode argument (default: confetti rain).
pub fn confetti_mode_from_args(args: &str) -> &'static str {
    match args.trim().to_ascii_lowercase().as_str() {
        "firework" | "fireworks" => "firework",
        _ => "confetti",
    }
}

/// Overlay slash commands that run immediately when confirmed from the palette.
pub fn slash_palette_submit_on_enter(command_name: &str) -> bool {
    matches!(command_name, "model" | "tree" | "resume")
}

fn builtin_dispatch(name: &str, args: String) -> Option<SlashDispatch> {
    match name {
        "exit" | "quit" | "q" => Some(SlashDispatch::Quit),
        "compact" | "c" => Some(SlashDispatch::Compact),
        "goal" | "goals" => Some(SlashDispatch::Goal { args }),
        "help" | "h" | "?" => Some(SlashDispatch::Help),
        "tools" => Some(SlashDispatch::Tools { args }),
        "system-prompt" | "systemprompt" | "prompt" => Some(SlashDispatch::SystemPrompt),
        "confetti" | "conffety" | "confetty" => Some(SlashDispatch::Confetti { args }),
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
    skills: Option<&[Skill]>,
) -> Option<SlashDispatch> {
    let trimmed = input.trim();
    if !trimmed.starts_with('/') {
        return None;
    }
    let body = trimmed.trim_start_matches('/').trim();
    if body.is_empty() {
        return None;
    }

    if let Some((name, args)) = parse_skill_slash(body) {
        if skills.is_some_and(|items| items.iter().any(|skill| skill.name == name)) {
            return Some(SlashDispatch::Skill { name, args });
        }
        return Some(SlashDispatch::Unimplemented(format!("/skill:{name}")));
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

    fn sample_skill() -> Skill {
        Skill {
            name: "code-review".into(),
            description: "Review changes".into(),
            content: "Review the code".into(),
            file_path: "/tmp/code-review/SKILL.md".into(),
            ..Default::default()
        }
    }

    #[test]
    fn provider_subcommands_are_unimplemented() {
        assert_eq!(
            dispatch_slash_command("/provider connect", None, None, None),
            Some(SlashDispatch::Unimplemented("/provider".into()))
        );
        assert_eq!(
            dispatch_slash_command("/provider connect anthropic", None, None, None),
            Some(SlashDispatch::Unimplemented("/provider".into()))
        );
    }

    #[test]
    fn wired_commands_dispatch() {
        assert_eq!(dispatch_slash_command("/exit", None, None, None), Some(SlashDispatch::Quit));
        assert_eq!(
            dispatch_slash_command("/compact", None, None, None),
            Some(SlashDispatch::Compact)
        );
        assert_eq!(
            dispatch_slash_command("/goal pause", None, None, None),
            Some(SlashDispatch::Goal { args: "pause".into() })
        );
        assert_eq!(dispatch_slash_command("/help", None, None, None), Some(SlashDispatch::Help));
        assert_eq!(
            dispatch_slash_command("/tools", None, None, None),
            Some(SlashDispatch::Tools { args: String::new() })
        );
        assert_eq!(
            dispatch_slash_command("/tools json", None, None, None),
            Some(SlashDispatch::Tools { args: "json".into() })
        );
        assert_eq!(
            dispatch_slash_command("/tools table", None, None, None),
            Some(SlashDispatch::Tools { args: "table".into() })
        );
        assert_eq!(
            dispatch_slash_command("/system-prompt", None, None, None),
            Some(SlashDispatch::SystemPrompt)
        );
        assert_eq!(
            dispatch_slash_command("/prompt", None, None, None),
            Some(SlashDispatch::SystemPrompt)
        );
        assert_eq!(dispatch_slash_command("/reload", None, None, None), Some(SlashDispatch::Reload));
    }

    #[test]
    fn overlay_commands_dispatch() {
        assert_eq!(
            dispatch_slash_command("/model", None, None, None),
            Some(SlashDispatch::OverlayNeeded(OverlayCommand::Model { filter: String::new() }))
        );
        assert_eq!(
            dispatch_slash_command("/model ", None, None, None),
            Some(SlashDispatch::OverlayNeeded(OverlayCommand::Model { filter: String::new() }))
        );
        assert_eq!(
            dispatch_slash_command("/model opus", None, None, None),
            Some(SlashDispatch::OverlayNeeded(OverlayCommand::Model { filter: "opus".into() }))
        );
        assert_eq!(
            dispatch_slash_command("/tree", None, None, None),
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
            dispatch_slash_command("/review main.rs", None, Some(&templates), None),
            Some(SlashDispatch::PromptTemplate {
                name: "review".into(),
                args: "main.rs".into()
            })
        );
    }

    #[test]
    fn skill_slash_dispatch() {
        let skills = vec![sample_skill()];
        assert_eq!(
            dispatch_slash_command("/skill:code-review src/", None, None, Some(&skills)),
            Some(SlashDispatch::Skill {
                name: "code-review".into(),
                args: "src/".into()
            })
        );
        assert_eq!(
            dispatch_slash_command("/skill:missing", None, None, Some(&skills)),
            Some(SlashDispatch::Unimplemented("/skill:missing".into()))
        );
    }

    #[test]
    fn palette_lists_skill_commands_with_prefix() {
        let skills = vec![sample_skill()];
        let names: Vec<_> = slash_commands_for_palette(None, None, Some(&skills))
            .into_iter()
            .map(|cmd| cmd.name)
            .collect();
        assert!(names.contains(&"skill:code-review".to_string()));
    }

    #[test]
    fn palette_includes_tools_args_hint() {
        let commands = slash_commands_for_palette(None, None, None);
        let tools = commands.iter().find(|cmd| cmd.name == "tools").expect("tools");
        assert_eq!(tools.args_hint.as_deref(), Some("[json|list|table]"));
        assert_eq!(tools.palette_command_label(), "/tools [json|list|table]");
        assert_eq!(tools.description, "Show active tools");
    }

    #[test]
    fn slash_arg_completions_cover_tools_and_goal() {
        assert!(slash_arg_completions("tools").is_some());
        assert!(slash_arg_completions("goal").is_some());
        assert!(slash_arg_completions("model").is_none());
    }

    #[test]
    fn palette_lists_goal_and_provider() {
        let names: Vec<_> = builtin_slash_commands().into_iter().map(|cmd| cmd.name).collect();
        assert!(names.contains(&"goal"));
        assert!(names.contains(&"provider"));
    }

    #[test]
    fn hidden_commands_dispatch_but_skip_palette() {
        assert_eq!(
            dispatch_slash_command("/confetti", None, None, None),
            Some(SlashDispatch::Confetti { args: String::new() })
        );
        assert_eq!(
            dispatch_slash_command("/confetti firework", None, None, None),
            Some(SlashDispatch::Confetti {
                args: "firework".into()
            })
        );
        assert_eq!(
            dispatch_slash_command("/conffety", None, None, None),
            Some(SlashDispatch::Confetti { args: String::new() })
        );
        assert_eq!(confetti_mode_from_args(""), "confetti");
        assert_eq!(confetti_mode_from_args("fireworks"), "firework");

        let names: Vec<_> = slash_commands_for_palette(None, None, None)
            .into_iter()
            .map(|cmd| cmd.name)
            .collect();
        assert!(!names.iter().any(|name| name == "confetti"));

        let help = format_help_message(None, None, None);
        assert!(!help.contains("/confetti"));
    }

    #[test]
    fn palette_skips_template_names_that_match_builtins() {
        let templates = vec![PromptTemplate {
            name: "help".into(),
            description: "Custom help".into(),
            content: "Help me".into(),
        }];
        let names: Vec<_> = slash_commands_for_palette(None, Some(&templates), None)
            .into_iter()
            .map(|cmd| cmd.name)
            .collect();
        assert_eq!(names.iter().filter(|name| **name == "help").count(), 1);
    }
}
