//! Built-in slash command registry and dispatch.

use crate::agent::{MAX_PALETTE_DESCRIPTION_CHARS, parse_skill_slash, skill_slash_name, truncate_palette_description};
use crate::types::{SlashCommand, SlashCommandKind};
use elph_agent::harness::{PromptTemplate, Skill};

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

fn builtin_with_args(name: &'static str, description: &'static str) -> BuiltinSlashCommand {
    BuiltinSlashCommand {
        name,
        description,
        args_hint: Some("[args]"),
        hidden: false,
    }
}

fn hidden_builtin_with_args(name: &'static str, description: &'static str) -> BuiltinSlashCommand {
    BuiltinSlashCommand {
        name,
        description,
        args_hint: Some("[args]"),
        hidden: true,
    }
}

fn hidden_builtin(name: &'static str, description: &'static str) -> BuiltinSlashCommand {
    BuiltinSlashCommand {
        name,
        description,
        args_hint: None,
        hidden: true,
    }
}

pub fn builtin_slash_commands() -> Vec<BuiltinSlashCommand> {
    vec![
        builtin("settings", "Open settings menu"),
        builtin_with_args("model", "Select model"),
        builtin("thinking", "Select thinking level"),
        builtin("scoped-models", "Enable models for Ctrl+P cycling"),
        builtin("export", "Export session (JSONL)"),
        builtin("import", "Import session JSONL"),
        builtin("rename", "Rename the current session"),
        builtin("session", "Show session info"),
        builtin("changelog", "Show changelog"),
        builtin("hotkeys", "Show keyboard shortcuts"),
        builtin("fork", "Fork from a message"),
        builtin("clone", "Clone current session"),
        builtin("tree", "Navigate session tree"),
        builtin("trust", "Save project trust decision"),
        builtin_with_args("provider", "Manage providers"),
        builtin_with_args("transfer", "Resume a foreign coding-agent session"),
        builtin_with_args("mcp", "MCP servers"),
        builtin("new", "Start a new session"),
        builtin_with_args("compact", "Compact conversation history"),
        builtin("continue", "Resume the interrupted task"),
        builtin("resume", "Resume a different session"),
        builtin("workers", "List live multi-worker peers"),
        builtin("intercom", "Open worker chat (Alt+M)"),
        builtin("reload", "Reload providers, settings, skills, and templates"),
        builtin("quit", "Quit Elph"),
        builtin_with_args("memory", "Agent memory store (floppy)"),
        builtin("feedback", "Report a bug or join community"),
        builtin("help", "List commands"),
        builtin_with_args("aside", "Ask a side question without interrupting"),
        builtin("tools", "Show active tools"),
        builtin("view-plan", "Preview the saved plan"),
        builtin("system-prompt", "Show compiled system prompt"),
        builtin("exit", "Quit Elph"),
        builtin_with_args("goal", "Manage session goals"),
        hidden_builtin_with_args("confetti", "Confetti celebration"),
        hidden_builtin("login", "Sign in to an AI provider"),
        hidden_builtin("logout", "Sign out from an AI provider"),
    ]
}

pub fn slash_commands_for_palette(
    prompt_templates: Option<&[PromptTemplate]>,
    skills: Option<&[Skill]>,
) -> Vec<SlashCommand> {
    slash_commands_for_palette_with(prompt_templates, skills, true)
}

pub fn slash_commands_for_palette_with(
    prompt_templates: Option<&[PromptTemplate]>,
    skills: Option<&[Skill]>,
    enable_skill_commands: bool,
) -> Vec<SlashCommand> {
    // Include hidden builtins (e.g. `/confetti`) so Tab can still complete them when the
    // typed query matches. Empty-query palette + `/help` filter them out via `hidden`.
    let mut commands: Vec<SlashCommand> = builtin_slash_commands()
        .into_iter()
        .map(|cmd| {
            let mut entry = SlashCommand::new(cmd.name, truncate_palette_description(cmd.description, None));
            if let Some(hint) = cmd.args_hint {
                entry = entry.with_args_hint(hint);
            }
            if cmd.hidden {
                entry = entry.with_hidden(true);
            }
            entry
        })
        .collect();
    let builtin_names: std::collections::HashSet<String> = commands.iter().map(|cmd| cmd.name.clone()).collect();

    if let Some(templates) = prompt_templates {
        for template in templates {
            if !builtin_names.contains(&template.name) {
                let mut cmd = SlashCommand::new(
                    &template.name,
                    format!("[prompt] {}", truncate_palette_description(&template.description, None)),
                )
                .with_kind(SlashCommandKind::PromptTemplate);
                if let Some(hint) = &template.argument_hint {
                    cmd = cmd.with_args_hint(hint);
                }
                commands.push(cmd);
            }
        }
    }
    if enable_skill_commands && let Some(skills) = skills {
        for skill in skills {
            let name = skill_slash_name(&skill.name);
            if !builtin_names.contains(&name) {
                let mut cmd = SlashCommand::new(
                    name,
                    format!("[skill] {}", truncate_palette_description(&skill.description, None)),
                )
                .with_kind(SlashCommandKind::Skill);
                if let Some(hint) = &skill.argument_hint {
                    cmd = cmd.with_args_hint(hint);
                }
                commands.push(cmd);
            }
        }
    }
    commands.sort_by(|a, b| a.name.cmp(&b.name));
    commands
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OverlayCommand {
    Model { filter: String },
    ScopedModels,
    Tree,
    Resume,
    ProviderConnect { provider_id: Option<String> },
}

/// Options for the `/compact` slash command.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CompactOptions {
    /// Override compaction threshold percentage (1-100).
    pub threshold_pct: Option<u8>,
    /// Override tokens to keep after compaction.
    pub keep_recent_tokens: Option<u64>,
    /// Override model for summarization (e.g., "openai/gpt-4").
    pub model: Option<String>,
    /// Enable memory flush before compaction.
    pub memory_flush: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlashDispatch {
    Quit,
    NewSession,
    Compact {
        options: CompactOptions,
    },
    /// Submit the `RETRY_CONTINUE_PROMPT` recovery prompt (`/continue`) so the model
    /// resumes an interrupted task without re-doing completed tool work. Rendered as a
    /// slim "Continuing tasks…" meta line rather than a user prompt card.
    Continue,
    Goal {
        args: String,
    },
    Help,
    Tools {
        args: String,
    },
    SystemPrompt,
    /// Open the latest saved plan preview.
    ViewPlan,
    /// Show current session metadata (title, id, model, context, …).
    SessionInfo,
    /// Open rename dialog; `args` is optional prefill when non-empty.
    Rename {
        args: String,
    },
    Confetti {
        args: String,
    },
    Reload,
    PromptTemplate {
        name: String,
        args: String,
    },
    Skill {
        name: String,
        args: String,
    },
    /// Memory store commands (status, list, tasks, log, search, purge, flush).
    Memory {
        args: String,
    },
    OverlayNeeded(OverlayCommand),
    /// Open feedback dialog (Report a Bug / Join Community).
    Feedback,
    /// Open provider connection dialog with OAuth or API key input.
    ProviderConnect {
        provider_id: Option<String>,
    },
    /// Open provider disconnect dialog to remove stored credentials.
    ProviderDisconnect {
        provider_id: Option<String>,
    },
    /// List configured providers in the transcript.
    ProviderList,
    /// Update provider model catalogs from the embedded seed (`/provider update [id]`).
    ProviderUpdate {
        provider_id: Option<String>,
    },
    /// Open MCP OAuth dialog (`/mcp auth [name]`).
    McpAuth {
        server_name: Option<String>,
    },
    /// Clear MCP OAuth credentials (`/mcp logout [name]`).
    McpLogout {
        server_name: Option<String>,
    },
    /// List MCP servers in the transcript (`/mcp list`).
    McpList,
    /// Resume a foreign coding-agent session (`/transfer claude [ref]`).
    ///
    /// `args` is the raw slash body after `/transfer ` — the first token selects
    /// the source tool (`claude` or `codex`), the rest is a session reference
    /// (empty / `latest` / session UUID / free-text title).
    Transfer {
        args: String,
    },
    /// Live multi-worker peers (`/workers`).
    Workers,
    /// Open worker chat (`/intercom`, Alt+M).
    WorkerChat,
    /// Keyboard shortcut reference (`/hotkeys`).
    Hotkeys,
    /// Changelog text (`/changelog`).
    Changelog,
    /// Settings paths and tips (`/settings`).
    Settings,
    /// Export session branch as JSONL (`/export [path]`).
    Export {
        args: String,
    },
    /// Import session JSONL (`/import [path]`).
    Import {
        args: String,
    },
    /// Mark project trusted (`/trust`).
    Trust,
    /// Fork current session (`/fork`).
    Fork,
    /// Clone current session (`/clone`).
    CloneSession,
    /// Session tree inspect / navigate (`/tree [entry_id] [--summary]`).
    Tree {
        args: String,
    },
    /// List sessions or switch (`/resume [id]`).
    Resume {
        args: String,
    },
    /// Side question that does not interrupt the main turn (`/aside <question>`).
    Aside {
        question: String,
    },
    /// Open the thinking level picker (`/thinking`).
    Thinking,
    Unimplemented(String),
}

pub fn slash_unimplemented_message(command: &str) -> String {
    let name = command.trim_start_matches('/').trim();
    format!("{name} not yet implemented")
}

pub fn format_help_message(prompt_templates: Option<&[PromptTemplate]>, skills: Option<&[Skill]>) -> String {
    let commands = slash_commands_for_palette(prompt_templates, skills);
    let mut lines = vec!["Slash commands:".to_string()];
    for cmd in commands.into_iter().filter(|cmd| !cmd.hidden) {
        // `/help` is rendered as plain text (no box), so cap the description here.
        let desc = truncate_palette_description(&cmd.description, Some(MAX_PALETTE_DESCRIPTION_CHARS));
        lines.push(format!("  {} — {}", cmd.palette_command_label(), desc));
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

const MEMORY_ARG_COMPLETIONS: &[SlashArgCompletion] = &[
    SlashArgCompletion {
        value: "status",
        description: "Show memory status",
    },
    SlashArgCompletion {
        value: "list",
        description: "List memory entries",
    },
    SlashArgCompletion {
        value: "recent",
        description: "Newest memories first",
    },
    SlashArgCompletion {
        value: "tasks",
        description: "Show memory tasks",
    },
    SlashArgCompletion {
        value: "log",
        description: "Show memory log",
    },
    SlashArgCompletion {
        value: "search",
        description: "Search memory",
    },
    SlashArgCompletion {
        value: "purge",
        description: "Delete weak memories by weight",
    },
    SlashArgCompletion {
        value: "flush",
        description: "Wipe entire store (confirm first)",
    },
    SlashArgCompletion {
        value: "consolidate",
        description: "Merge near-duplicate entries",
    },
];

const PROVIDER_ARG_COMPLETIONS: &[SlashArgCompletion] = &[
    SlashArgCompletion {
        value: "connect",
        description: "Connect to an AI provider",
    },
    SlashArgCompletion {
        value: "disconnect",
        description: "Disconnect from an AI provider",
    },
    SlashArgCompletion {
        value: "list",
        description: "List configured providers",
    },
    SlashArgCompletion {
        value: "update",
        description: "Update model catalogs from the embedded seed",
    },
];

const MCP_ARG_COMPLETIONS: &[SlashArgCompletion] = &[
    SlashArgCompletion {
        value: "auth",
        description: "OAuth login for a remote MCP server",
    },
    SlashArgCompletion {
        value: "logout",
        description: "Clear OAuth credentials for an MCP server",
    },
    SlashArgCompletion {
        value: "list",
        description: "List configured MCP servers",
    },
];

const TRANSFER_ARG_COMPLETIONS: &[SlashArgCompletion] = &[
    SlashArgCompletion {
        value: "claude",
        description: "Resume work from a Claude Code session",
    },
    SlashArgCompletion {
        value: "codex",
        description: "Resume work from a Codex session",
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
        "tools" => None,
        "goal" | "goals" => Some(GOAL_ARG_COMPLETIONS),
        "confetti" | "conffety" | "confetty" => Some(CONFETTI_ARG_COMPLETIONS),
        "memory" | "mem" => Some(MEMORY_ARG_COMPLETIONS),
        "provider" => Some(PROVIDER_ARG_COMPLETIONS),
        "mcp" => Some(MCP_ARG_COMPLETIONS),
        "transfer" => Some(TRANSFER_ARG_COMPLETIONS),
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

/// Parse `/compact` command arguments.
///
/// Supported args:
/// - `--threshold <pct>`: Override compaction threshold (1-100)
/// - `--keep-recent <tokens>`: Override tokens to keep
/// - `--model <model>`: Override model for summarization
/// - `--memory-flush`: Enable memory flush before compaction
fn parse_compact_args(args: &str) -> CompactOptions {
    let mut options = CompactOptions::default();
    let mut tokens = args.split_whitespace().peekable();

    while let Some(token) = tokens.next() {
        match token {
            "--threshold" => {
                if let Some(value) = tokens.next()
                    && let Ok(pct) = value.parse::<u8>()
                {
                    options.threshold_pct = Some(pct.clamp(1, 100));
                }
            }
            "--keep-recent" => {
                if let Some(value) = tokens.next()
                    && let Ok(tokens) = value.parse::<u64>()
                {
                    options.keep_recent_tokens = Some(tokens);
                }
            }
            "--model" => {
                if let Some(value) = tokens.next() {
                    options.model = Some(value.to_string());
                }
            }
            "--memory-flush" => {
                options.memory_flush = true;
            }
            _ => {
                // Ignore unknown args
            }
        }
    }

    options
}

#[cfg(test)]
mod compact_args_tests {
    use super::*;

    #[test]
    fn parse_compact_args_empty() {
        let opts = parse_compact_args("");
        assert_eq!(opts.threshold_pct, None);
        assert_eq!(opts.keep_recent_tokens, None);
        assert_eq!(opts.model, None);
        assert!(!opts.memory_flush);
    }

    #[test]
    fn parse_compact_args_threshold() {
        let opts = parse_compact_args("--threshold 85");
        assert_eq!(opts.threshold_pct, Some(85));
        assert_eq!(opts.keep_recent_tokens, None);
        assert_eq!(opts.model, None);
        assert!(!opts.memory_flush);
    }

    #[test]
    fn parse_compact_args_threshold_clamped() {
        let opts = parse_compact_args("--threshold 150");
        assert_eq!(opts.threshold_pct, Some(100)); // clamped to max
    }

    #[test]
    fn parse_compact_args_keep_recent() {
        let opts = parse_compact_args("--keep-recent 20000");
        assert_eq!(opts.threshold_pct, None);
        assert_eq!(opts.keep_recent_tokens, Some(20000));
        assert_eq!(opts.model, None);
        assert!(!opts.memory_flush);
    }

    #[test]
    fn parse_compact_args_model() {
        let opts = parse_compact_args("--model openai/gpt-4");
        assert_eq!(opts.threshold_pct, None);
        assert_eq!(opts.keep_recent_tokens, None);
        assert_eq!(opts.model, Some("openai/gpt-4".to_string()));
        assert!(!opts.memory_flush);
    }

    #[test]
    fn parse_compact_args_memory_flush() {
        let opts = parse_compact_args("--memory-flush");
        assert_eq!(opts.threshold_pct, None);
        assert_eq!(opts.keep_recent_tokens, None);
        assert_eq!(opts.model, None);
        assert!(opts.memory_flush);
    }

    #[test]
    fn parse_compact_args_multiple() {
        let opts = parse_compact_args("--threshold 90 --keep-recent 15000 --model anthropic/claude-3 --memory-flush");
        assert_eq!(opts.threshold_pct, Some(90));
        assert_eq!(opts.keep_recent_tokens, Some(15000));
        assert_eq!(opts.model, Some("anthropic/claude-3".to_string()));
        assert!(opts.memory_flush);
    }

    #[test]
    fn parse_compact_args_unknown_ignored() {
        let opts = parse_compact_args("--unknown foo --threshold 80");
        assert_eq!(opts.threshold_pct, Some(80));
        assert_eq!(opts.keep_recent_tokens, None);
        assert_eq!(opts.model, None);
        assert!(!opts.memory_flush);
    }

    #[test]
    fn parse_compact_args_invalid_value_ignored() {
        let opts = parse_compact_args("--threshold abc");
        assert_eq!(opts.threshold_pct, None);
    }
}

/// Overlay slash commands that run immediately when confirmed from the palette.
///
/// Commands with arg completions (e.g. `tools`, `goal`, `confetti`) are **not** listed —
/// Tab/Enter first complete the name so the args palette can open.
pub fn slash_palette_submit_on_enter(command_name: &str) -> bool {
    matches!(
        command_name,
        "model"
            | "scoped-models"
            | "thinking"
            | "tree"
            | "resume"
            | "session"
            | "rename"
            | "system-prompt"
            | "feedback"
            | "workers"
            | "intercom"
            | "hotkeys"
            | "changelog"
            | "settings"
            | "trust"
            | "fork"
            | "clone"
    )
}

fn builtin_dispatch(name: &str, args: String) -> Option<SlashDispatch> {
    match name {
        "exit" | "quit" | "q" => Some(SlashDispatch::Quit),
        "compact" | "c" => Some(SlashDispatch::Compact {
            options: parse_compact_args(&args),
        }),
        "continue" | "cont" => Some(SlashDispatch::Continue),
        "goal" | "goals" => Some(SlashDispatch::Goal { args }),
        "help" | "h" | "?" => Some(SlashDispatch::Help),
        "aside" => Some(SlashDispatch::Aside { question: args }),
        "tools" => Some(SlashDispatch::Tools { args }),
        "system-prompt" | "systemprompt" | "prompt" => Some(SlashDispatch::SystemPrompt),
        "view-plan" | "show-plan" | "plan-view" => Some(SlashDispatch::ViewPlan),
        "session" => Some(SlashDispatch::SessionInfo),
        "rename" | "name" => Some(SlashDispatch::Rename { args }),
        "confetti" | "conffety" | "confetty" => Some(SlashDispatch::Confetti { args }),
        "reload" => Some(SlashDispatch::Reload),
        "model" => Some(SlashDispatch::OverlayNeeded(OverlayCommand::Model { filter: args })),
        "thinking" | "think" => Some(SlashDispatch::Thinking),
        "scoped-models" | "scoped_models" | "scopedmodels" => {
            Some(SlashDispatch::OverlayNeeded(OverlayCommand::ScopedModels))
        }
        "tree" => Some(SlashDispatch::Tree { args }),
        "resume" => Some(SlashDispatch::Resume { args }),
        "new" => Some(SlashDispatch::NewSession),
        "feedback" => Some(SlashDispatch::Feedback),
        "memory" | "mem" => Some(SlashDispatch::Memory { args }),
        "login" => Some(SlashDispatch::ProviderConnect { provider_id: None }),
        "logout" => Some(SlashDispatch::ProviderDisconnect { provider_id: None }),
        "workers" => Some(SlashDispatch::Workers),
        "intercom" | "ic" => Some(SlashDispatch::WorkerChat),
        "hotkeys" | "keys" | "shortcuts" => Some(SlashDispatch::Hotkeys),
        "changelog" | "changes" => Some(SlashDispatch::Changelog),
        "settings" | "config" => Some(SlashDispatch::Settings),
        "export" => Some(SlashDispatch::Export { args }),
        "import" => Some(SlashDispatch::Import { args }),
        "trust" => Some(SlashDispatch::Trust),
        "fork" => Some(SlashDispatch::Fork),
        "clone" => Some(SlashDispatch::CloneSession),
        "copy" => Some(SlashDispatch::CloneSession),
        "provider" => {
            if args.trim().is_empty() {
                Some(SlashDispatch::ProviderConnect { provider_id: None })
            } else if args.starts_with("connect") {
                let provider_id = args.trim_start_matches("connect").trim().to_string();
                let provider_id = if provider_id.is_empty() {
                    None
                } else {
                    Some(provider_id)
                };
                Some(SlashDispatch::ProviderConnect { provider_id })
            } else if args.starts_with("disconnect") {
                let provider_id = args.trim_start_matches("disconnect").trim().to_string();
                let provider_id = if provider_id.is_empty() {
                    None
                } else {
                    Some(provider_id)
                };
                Some(SlashDispatch::ProviderDisconnect { provider_id })
            } else if args.starts_with("list") || args.trim() == "ls" {
                Some(SlashDispatch::ProviderList)
            } else if args.starts_with("update") {
                let provider_id = args.trim_start_matches("update").trim().to_string();
                let provider_id = if provider_id.is_empty() {
                    None
                } else {
                    Some(provider_id)
                };
                Some(SlashDispatch::ProviderUpdate { provider_id })
            } else {
                Some(SlashDispatch::Unimplemented(format!("/provider {args}")))
            }
        }
        "mcp" => {
            let args = args.trim();
            if args.is_empty() || args == "list" || args == "ls" {
                Some(SlashDispatch::McpList)
            } else if let Some(rest) = args
                .strip_prefix("auth")
                .or_else(|| args.strip_prefix("login"))
                .or_else(|| args.strip_prefix("connect"))
            {
                let rest = rest.trim();
                let server_name = if rest.is_empty() { None } else { Some(rest.to_string()) };
                Some(SlashDispatch::McpAuth { server_name })
            } else if let Some(rest) = args.strip_prefix("logout").or_else(|| args.strip_prefix("disconnect")) {
                let rest = rest.trim();
                let server_name = if rest.is_empty() { None } else { Some(rest.to_string()) };
                Some(SlashDispatch::McpLogout { server_name })
            } else {
                Some(SlashDispatch::Unimplemented(format!("/mcp {args}")))
            }
        }
        "transfer" => Some(SlashDispatch::Transfer { args }),
        _ => None,
    }
}

pub fn dispatch_slash_command(
    input: &str,
    prompt_templates: Option<&[PromptTemplate]>,
    skills: Option<&[Skill]>,
) -> Option<SlashDispatch> {
    dispatch_slash_command_with(input, prompt_templates, skills, true)
}

pub fn dispatch_slash_command_with(
    input: &str,
    prompt_templates: Option<&[PromptTemplate]>,
    skills: Option<&[Skill]>,
    enable_skill_commands: bool,
) -> Option<SlashDispatch> {
    let trimmed = input.trim();
    if !trimmed.starts_with('/') {
        return None;
    }
    let body = trimmed.trim_start_matches('/').trim();
    if body.is_empty() {
        return None;
    }

    // Legacy `/skill:<name>` prefix (backward-compat).
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

    if let Some(templates) = prompt_templates
        && templates.iter().any(|template| template.name == name)
    {
        return Some(SlashDispatch::PromptTemplate { name, args });
    }

    // Match skill by raw name (no prefix needed) when skill slash commands are enabled.
    if enable_skill_commands
        && let Some(skills) = skills
        && skills.iter().any(|skill| skill.name == name)
    {
        return Some(SlashDispatch::Skill { name, args });
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
    fn provider_connect_dispatch() {
        assert_eq!(
            dispatch_slash_command("/provider connect", None, None),
            Some(SlashDispatch::ProviderConnect { provider_id: None })
        );
        assert_eq!(
            dispatch_slash_command("/provider connect anthropic", None, None),
            Some(SlashDispatch::ProviderConnect {
                provider_id: Some("anthropic".to_string())
            })
        );
        assert_eq!(
            dispatch_slash_command("/provider", None, None),
            Some(SlashDispatch::ProviderConnect { provider_id: None })
        );
    }

    #[test]
    fn mcp_auth_dispatch() {
        assert_eq!(
            dispatch_slash_command("/mcp auth", None, None),
            Some(SlashDispatch::McpAuth { server_name: None })
        );
        assert_eq!(
            dispatch_slash_command("/mcp auth figma", None, None),
            Some(SlashDispatch::McpAuth {
                server_name: Some("figma".to_string())
            })
        );
        assert_eq!(dispatch_slash_command("/mcp list", None, None), Some(SlashDispatch::McpList));
        assert_eq!(
            dispatch_slash_command("/mcp logout figma", None, None),
            Some(SlashDispatch::McpLogout {
                server_name: Some("figma".to_string())
            })
        );
    }

    #[test]
    fn transfer_dispatch_and_completions() {
        assert_eq!(
            dispatch_slash_command("/transfer claude", None, None),
            Some(SlashDispatch::Transfer { args: "claude".into() })
        );
        assert_eq!(
            dispatch_slash_command("/transfer claude latest", None, None),
            Some(SlashDispatch::Transfer {
                args: "claude latest".into()
            })
        );
        assert_eq!(
            dispatch_slash_command("/transfer codex", None, None),
            Some(SlashDispatch::Transfer { args: "codex".into() })
        );
        assert_eq!(
            dispatch_slash_command("/transfer", None, None),
            Some(SlashDispatch::Transfer { args: String::new() })
        );
        let completions = slash_arg_completions("transfer").expect("transfer completions");
        assert!(completions.iter().any(|c| c.value == "claude"));
        assert!(completions.iter().any(|c| c.value == "codex"));
        let commands = slash_commands_for_palette(None, None);
        let transfer = commands.iter().find(|cmd| cmd.name == "transfer").expect("transfer");
        assert_eq!(transfer.args_hint.as_deref(), Some("[args]"));
        assert!(!transfer.hidden);
    }

    #[test]
    fn wired_commands_dispatch() {
        assert_eq!(dispatch_slash_command("/exit", None, None), Some(SlashDispatch::Quit));
        assert_eq!(
            dispatch_slash_command("/compact", None, None),
            Some(SlashDispatch::Compact {
                options: CompactOptions::default()
            })
        );
        assert_eq!(dispatch_slash_command("/continue", None, None), Some(SlashDispatch::Continue));
        assert_eq!(dispatch_slash_command("/cont", None, None), Some(SlashDispatch::Continue));
        assert_eq!(
            dispatch_slash_command("/goal pause", None, None),
            Some(SlashDispatch::Goal { args: "pause".into() })
        );
        assert_eq!(dispatch_slash_command("/help", None, None), Some(SlashDispatch::Help));
        assert_eq!(
            dispatch_slash_command("/aside is this safe?", None, None),
            Some(SlashDispatch::Aside {
                question: "is this safe?".into()
            })
        );
        assert_eq!(
            dispatch_slash_command("/aside", None, None),
            Some(SlashDispatch::Aside {
                question: String::new()
            })
        );
        assert_eq!(
            dispatch_slash_command("/tools", None, None),
            Some(SlashDispatch::Tools { args: String::new() })
        );
        assert_eq!(
            dispatch_slash_command("/tools table", None, None),
            Some(SlashDispatch::Tools { args: "table".into() })
        );
        assert_eq!(
            dispatch_slash_command("/tools list", None, None),
            Some(SlashDispatch::Tools { args: "list".into() })
        );
        assert_eq!(
            dispatch_slash_command("/system-prompt", None, None),
            Some(SlashDispatch::SystemPrompt)
        );
        assert_eq!(dispatch_slash_command("/prompt", None, None), Some(SlashDispatch::SystemPrompt));
        assert_eq!(dispatch_slash_command("/reload", None, None), Some(SlashDispatch::Reload));
        assert_eq!(dispatch_slash_command("/new", None, None), Some(SlashDispatch::NewSession));
    }

    #[test]
    fn overlay_commands_dispatch() {
        assert_eq!(
            dispatch_slash_command("/model", None, None),
            Some(SlashDispatch::OverlayNeeded(OverlayCommand::Model { filter: String::new() }))
        );
        assert_eq!(
            dispatch_slash_command("/model ", None, None),
            Some(SlashDispatch::OverlayNeeded(OverlayCommand::Model { filter: String::new() }))
        );
        assert_eq!(
            dispatch_slash_command("/model opus", None, None),
            Some(SlashDispatch::OverlayNeeded(OverlayCommand::Model { filter: "opus".into() }))
        );
        assert_eq!(
            dispatch_slash_command("/tree", None, None),
            Some(SlashDispatch::Tree { args: String::new() })
        );
        assert_eq!(
            dispatch_slash_command("/tree abc --summary", None, None),
            Some(SlashDispatch::Tree {
                args: "abc --summary".into()
            })
        );
        assert_eq!(
            dispatch_slash_command("/resume", None, None),
            Some(SlashDispatch::Resume { args: String::new() })
        );
        assert_eq!(
            dispatch_slash_command("/resume abc123", None, None),
            Some(SlashDispatch::Resume { args: "abc123".into() })
        );
        assert_eq!(dispatch_slash_command("/thinking", None, None), Some(SlashDispatch::Thinking));
        assert_eq!(dispatch_slash_command("/think", None, None), Some(SlashDispatch::Thinking));
        assert_eq!(dispatch_slash_command("/workers", None, None), Some(SlashDispatch::Workers));
        assert_eq!(dispatch_slash_command("/intercom", None, None), Some(SlashDispatch::WorkerChat));
        assert_eq!(
            dispatch_slash_command("/intercom calm-fox", None, None),
            Some(SlashDispatch::WorkerChat)
        );
    }

    #[test]
    fn template_dispatch_without_dynamic_command() {
        let templates = vec![PromptTemplate {
            name: "review".into(),
            description: "Review code".into(),
            content: "Review $@".into(),
            argument_hint: None,
            file_path: "/tmp/review.md".into(),
        }];
        assert_eq!(
            dispatch_slash_command("/review main.rs", Some(&templates), None),
            Some(SlashDispatch::PromptTemplate {
                name: "review".into(),
                args: "main.rs".into()
            })
        );
    }

    #[test]
    fn skill_slash_dispatch() {
        let skills = vec![sample_skill()];
        // Legacy `/skill:name` prefix still works.
        assert_eq!(
            dispatch_slash_command("/skill:code-review src/", None, Some(&skills)),
            Some(SlashDispatch::Skill {
                name: "code-review".into(),
                args: "src/".into()
            })
        );
        assert_eq!(
            dispatch_slash_command("/skill:missing", None, Some(&skills)),
            Some(SlashDispatch::Unimplemented("/skill:missing".into()))
        );
    }

    #[test]
    fn skill_dispatch_by_raw_name() {
        let skills = vec![sample_skill()];
        assert_eq!(
            dispatch_slash_command("/code-review src/", None, Some(&skills)),
            Some(SlashDispatch::Skill {
                name: "code-review".into(),
                args: "src/".into()
            })
        );
        assert_eq!(
            dispatch_slash_command("/code-review", None, Some(&skills)),
            Some(SlashDispatch::Skill {
                name: "code-review".into(),
                args: String::new()
            })
        );
    }

    #[test]
    fn palette_lists_skill_commands_without_prefix() {
        let skills = vec![sample_skill()];
        let names: Vec<_> = slash_commands_for_palette(None, Some(&skills))
            .into_iter()
            .map(|cmd| cmd.name)
            .collect();
        assert!(names.contains(&"code-review".to_string()));
        let skill = slash_commands_for_palette(None, Some(&skills))
            .into_iter()
            .find(|cmd| cmd.name == "code-review")
            .expect("skill command");
        // sample_skill() has no argument_hint → no args_hint.
        assert_eq!(skill.args_hint, None);
        assert!(skill.description.starts_with("[skill]"));
    }

    #[test]
    fn palette_includes_tools_without_args() {
        let commands = slash_commands_for_palette(None, None);
        let tools = commands.iter().find(|cmd| cmd.name == "tools").expect("tools");
        assert_eq!(tools.args_hint, None);
        assert_eq!(tools.palette_command_label(), "/tools");
        assert_eq!(tools.description, "Show active tools");
    }

    #[test]
    fn slash_arg_completions_cover_goal_and_others() {
        assert!(slash_arg_completions("tools").is_none());
        assert!(slash_arg_completions("goal").is_some());
        assert!(slash_arg_completions("memory").is_some());
        assert!(slash_arg_completions("mem").is_some());
        assert!(slash_arg_completions("provider").is_some());
        assert!(slash_arg_completions("mcp").is_some());
        assert!(slash_arg_completions("model").is_none());
        let mcp = slash_arg_completions("mcp").unwrap();
        assert!(mcp.iter().any(|c| c.value == "auth"));
        assert!(mcp.iter().any(|c| c.value == "logout"));
        assert!(mcp.iter().any(|c| c.value == "list"));
    }

    #[test]
    fn palette_lists_goal_and_provider() {
        let names: Vec<_> = builtin_slash_commands().into_iter().map(|cmd| cmd.name).collect();
        assert!(names.contains(&"goal"));
        assert!(names.contains(&"provider"));
        assert!(names.contains(&"mcp"));
        assert!(names.contains(&"session"));
        assert!(names.contains(&"rename"));
    }

    #[test]
    fn mcp_palette_args_hint() {
        let commands = slash_commands_for_palette(None, None);
        let mcp = commands.iter().find(|cmd| cmd.name == "mcp").expect("mcp");
        assert_eq!(mcp.args_hint.as_deref(), Some("[args]"));
        assert_eq!(mcp.palette_command_label(), "/mcp [args]");
    }

    #[test]
    fn session_and_rename_dispatch() {
        assert_eq!(dispatch_slash_command("/session", None, None), Some(SlashDispatch::SessionInfo));
        assert_eq!(
            dispatch_slash_command("/rename", None, None),
            Some(SlashDispatch::Rename { args: String::new() })
        );
        assert_eq!(
            dispatch_slash_command("/rename Fix login", None, None),
            Some(SlashDispatch::Rename {
                args: "Fix login".into()
            })
        );
        assert_eq!(
            dispatch_slash_command("/name My title", None, None),
            Some(SlashDispatch::Rename {
                args: "My title".into()
            })
        );
    }

    #[test]
    fn hidden_commands_dispatch_but_skip_palette() {
        assert_eq!(
            dispatch_slash_command("/confetti", None, None),
            Some(SlashDispatch::Confetti { args: String::new() })
        );
        assert_eq!(
            dispatch_slash_command("/confetti firework", None, None),
            Some(SlashDispatch::Confetti {
                args: "firework".into()
            })
        );
        assert_eq!(
            dispatch_slash_command("/conffety", None, None),
            Some(SlashDispatch::Confetti { args: String::new() })
        );
        assert_eq!(confetti_mode_from_args(""), "confetti");
        assert_eq!(confetti_mode_from_args("fireworks"), "firework");

        let commands = slash_commands_for_palette(None, None);
        let confetti = commands.iter().find(|cmd| cmd.name == "confetti");
        // Kept in the registry (so Tab can match), but marked hidden for empty `/` + help.
        assert!(
            confetti.is_some_and(|cmd| cmd.hidden),
            "hidden commands stay in registry for Tab"
        );

        let help = format_help_message(None, None);
        assert!(!help.contains("/confetti"));
    }

    #[test]
    fn palette_skips_template_names_that_match_builtins() {
        let templates = vec![PromptTemplate {
            name: "help".into(),
            description: "Custom help".into(),
            content: "Help me".into(),
            argument_hint: None,
            file_path: "/tmp/help.md".into(),
        }];
        let names: Vec<_> = slash_commands_for_palette(Some(&templates), None)
            .into_iter()
            .map(|cmd| cmd.name)
            .collect();
        assert_eq!(names.iter().filter(|name| **name == "help").count(), 1);
    }
}
