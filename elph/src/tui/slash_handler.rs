//! Slash command outcomes for the TUI shell.

use std::path::Path;
use std::sync::Arc;

use elph_agent::{ExtensionRegistry, PromptTemplate, Skill};

use crate::agent::{OverlayCommand, SlashDispatch};
use crate::agent::{
    confetti_mode_from_args, dispatch_slash_command, format_help_message, session_info_slash_message,
    session_title_for_rename, slash_unimplemented_message, system_prompt_slash_message, tools_slash_message,
};
use crate::extensions::ExtensionHost;
use crate::platform::Paths;
use crate::tui::confetti::confetti_mode_from_slash_args;

use super::agent_bridge::SlashDispatcher;

/// Handle `/memory` slash commands as a background task.
///
/// Memory operations are async (Turso DB). The result is delivered as a
/// `MemoryResult` UI event so the shell can open a ScrollTextDialog.
///
/// `flush` is special: it opens a confirmation dialog instead of running
/// immediately (see [`SlashOutcome::OpenMemoryFlushConfirm`]).
fn handle_memory_slash(ctx: SlashContext<'_>, args: &str) -> SlashOutcome {
    let Some(paths) = ctx.paths else {
        return SlashOutcome::Status("Project directory required for memory commands.".into());
    };

    // Destructive wipe — confirm in the status-zone dialog first.
    if let Ok(crate::memory::ops::MemoryOp::Flush) = crate::memory::ops::MemoryOp::parse_slash(args) {
        let (memory_count, task_count) = match elph_agent::try_block_on(crate::memory::flush_preview(paths)) {
            Ok(counts) => counts,
            Err(err) => return SlashOutcome::Status(format!("Memory error: {err:#}")),
        };
        return SlashOutcome::OpenMemoryFlushConfirm {
            memory_count,
            task_count,
        };
    }

    let paths = paths.clone();
    let args = args.to_string();
    let ui_tx = ctx.agent_session.as_ref().map(|s| s.ui_event_sender());

    // Prefer async + dialog when the session UI channel exists.
    if let Some(tx) = ui_tx {
        tokio::spawn(async move {
            let output = match crate::memory::slash_run(&paths, &args).await {
                Ok(text) => text,
                Err(err) => format!("Memory error: {err}"),
            };
            let _ = tx.send(crate::agent::AgentUiEvent::MemoryResult(output));
        });
        return SlashOutcome::BackgroundTask;
    }

    // Fallback: run inline so output is never silently dropped.
    match elph_agent::try_block_on(crate::memory::slash_run(&paths, &args)) {
        Ok(Ok(text)) => SlashOutcome::OpenMemoryResultDialog { text },
        Ok(Err(err)) => SlashOutcome::Status(format!("Memory error: {err}")),
        Err(err) => SlashOutcome::Status(format!("Memory error: {err:#}")),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlashOutcome {
    Quit,
    NewSession,
    BackgroundTask,
    Status(String),
    Assistant(String),
    Unimplemented(String),
    SpawnAgentTurn,
    OverlayDeferred(OverlayCommand),
    OpenModelSelector {
        filter: String,
    },
    OpenScopedModels,
    OpenSystemPromptDialog {
        text: String,
    },
    /// Session metadata viewer (ScrollTextDialog).
    OpenSessionInfoDialog {
        text: String,
    },
    /// Provider list viewer (ScrollTextDialog).
    OpenProviderListDialog {
        text: String,
    },
    /// Memory operation result viewer (ScrollTextDialog).
    #[allow(dead_code)]
    OpenMemoryResultDialog {
        text: String,
    },
    /// Confirm wiping the entire memory store before executing flush.
    OpenMemoryFlushConfirm {
        memory_count: u32,
        task_count: u32,
    },
    /// Rename session inline text dialog (prefilled title).
    OpenRenameDialog {
        initial: String,
    },
    PlayConfetti {
        mode: crate::tui::confetti::ConfettiMode,
    },
    /// Open feedback dialog (Report a Bug / Join Community).
    OpenFeedbackDialog,
    /// Open provider connection dialog with OAuth or API key input.
    OpenProviderConnectDialog {
        provider_id: Option<String>,
    },
    /// Open provider disconnect dialog to remove stored credentials.
    OpenProviderDisconnectDialog {
        provider_id: Option<String>,
    },
}

pub struct SlashContext<'a> {
    pub input: &'a str,
    pub extensions: Option<&'a ExtensionRegistry>,
    pub prompt_templates: Option<&'a [PromptTemplate]>,
    pub skills: Option<&'a [Skill]>,
    pub agent_session: Option<Arc<crate::agent::CodingAgentSession>>,
    pub extension_host: Option<&'a ExtensionHost>,
    pub paths: Option<&'a Paths>,
    pub cwd: Option<&'a Path>,
    /// When false (agent busy), do not start skill/compact/etc. — caller queues or rejects.
    pub spawn_agent_work: bool,
}

pub fn handle_slash_submit(ctx: SlashContext<'_>) -> SlashOutcome {
    let Some(dispatch) = dispatch_slash_command(ctx.input, ctx.extensions, ctx.prompt_templates, ctx.skills) else {
        return SlashOutcome::SpawnAgentTurn;
    };

    // Memory commands run without an agent session — dispatch immediately.
    if let SlashDispatch::Memory { ref args } = dispatch {
        return handle_memory_slash(ctx, args);
    }

    match dispatch {
        SlashDispatch::Quit => SlashOutcome::Quit,
        SlashDispatch::NewSession => SlashOutcome::NewSession,
        SlashDispatch::Help => {
            SlashOutcome::Status(format_help_message(ctx.extensions, ctx.prompt_templates, ctx.skills))
        }
        SlashDispatch::Tools { args } => match tools_slash_message(ctx.agent_session.as_ref(), &args) {
            Ok(message) => SlashOutcome::Assistant(message),
            Err(message) => SlashOutcome::Status(message),
        },
        SlashDispatch::SystemPrompt => match system_prompt_slash_message(ctx.agent_session.as_ref()) {
            Ok(text) => SlashOutcome::OpenSystemPromptDialog { text },
            Err(message) => SlashOutcome::Status(message),
        },
        SlashDispatch::SessionInfo => match session_info_slash_message(ctx.agent_session.as_ref(), ctx.skills) {
            Ok(text) => SlashOutcome::OpenSessionInfoDialog { text },
            Err(message) => SlashOutcome::Status(message),
        },
        SlashDispatch::Rename { .. } => {
            if ctx.agent_session.is_none() {
                return SlashOutcome::Status("Agent session required for this command.".into());
            }
            let initial = session_title_for_rename(ctx.agent_session.as_ref()).unwrap_or_default();
            SlashOutcome::OpenRenameDialog { initial }
        }
        SlashDispatch::Confetti { args } => SlashOutcome::PlayConfetti {
            mode: confetti_mode_from_slash_args(confetti_mode_from_args(&args)),
        },
        SlashDispatch::Feedback => SlashOutcome::OpenFeedbackDialog,
        SlashDispatch::ProviderConnect { provider_id } => SlashOutcome::OpenProviderConnectDialog { provider_id },
        SlashDispatch::ProviderDisconnect { provider_id } => SlashOutcome::OpenProviderDisconnectDialog { provider_id },
        SlashDispatch::ProviderList => SlashOutcome::OpenProviderListDialog {
            text: provider_list_slash_message(),
        },
        // Handled by early return above — unreachable here.
        SlashDispatch::Memory { .. } => unreachable!(),
        SlashDispatch::Unimplemented(command) => SlashOutcome::Unimplemented(slash_unimplemented_message(&command)),
        SlashDispatch::OverlayNeeded(overlay) => match overlay {
            OverlayCommand::ProviderConnect { .. } => SlashOutcome::OverlayDeferred(overlay),
            OverlayCommand::Model { filter } => SlashOutcome::OpenModelSelector { filter },
            OverlayCommand::ScopedModels => SlashOutcome::OpenScopedModels,
            other => SlashOutcome::OverlayDeferred(other),
        },
        SlashDispatch::Compact | SlashDispatch::PromptTemplate { .. } => {
            if ctx.agent_session.is_none() {
                return SlashOutcome::Status("Agent session required for this command.".into());
            }
            if ctx.spawn_agent_work {
                let session = ctx.agent_session.clone().expect("checked above");
                let paths = ctx.paths.cloned();
                let cwd = ctx.cwd.map(|path| path.to_path_buf());
                let extension_host = ctx.extension_host.cloned();
                SlashDispatcher::spawn(session, dispatch, extension_host, paths, cwd);
            }
            SlashOutcome::SpawnAgentTurn
        }
        SlashDispatch::Goal { .. } | SlashDispatch::Reload | SlashDispatch::Extension { .. } => {
            if ctx.agent_session.is_none() {
                return SlashOutcome::Status("Agent session required for this command.".into());
            }
            if ctx.spawn_agent_work {
                let session = ctx.agent_session.clone().expect("checked above");
                let paths = ctx.paths.cloned();
                let cwd = ctx.cwd.map(|path| path.to_path_buf());
                let extension_host = ctx.extension_host.cloned();
                SlashDispatcher::spawn(session, dispatch, extension_host, paths, cwd);
            }
            SlashOutcome::BackgroundTask
        }
        SlashDispatch::Skill { ref name, ref args } => {
            if let Some(skills) = ctx.skills
                && let Some(skill) = skills.iter().find(|skill| skill.name == *name)
                && let Some(notice) = elph_agent::skill_args_validation_notice(skill, args)
            {
                return SlashOutcome::Status(notice);
            }
            if ctx.agent_session.is_none() {
                return SlashOutcome::Status("Agent session required for this command.".into());
            }
            if ctx.spawn_agent_work {
                let session = ctx.agent_session.clone().expect("checked above");
                let paths = ctx.paths.cloned();
                let cwd = ctx.cwd.map(|path| path.to_path_buf());
                let extension_host = ctx.extension_host.cloned();
                SlashDispatcher::spawn(session, dispatch, extension_host, paths, cwd);
            }
            SlashOutcome::SpawnAgentTurn
        }
    }
}

/// Outcomes that only touch UI / local state and never start an agent turn.
///
/// Safe to apply while `busy` (streaming); they must not call nested `try_block_on`
/// on the TUI runtime without isolation (see `try_block_on_detached`).
pub fn slash_outcome_is_ui_only(outcome: &SlashOutcome) -> bool {
    matches!(
        outcome,
        SlashOutcome::Status(_)
            | SlashOutcome::Assistant(_)
            | SlashOutcome::Unimplemented(_)
            | SlashOutcome::NewSession
            | SlashOutcome::BackgroundTask
            | SlashOutcome::OpenModelSelector { .. }
            | SlashOutcome::OpenScopedModels
            | SlashOutcome::OpenSystemPromptDialog { .. }
            | SlashOutcome::OpenSessionInfoDialog { .. }
            | SlashOutcome::OpenRenameDialog { .. }
            | SlashOutcome::PlayConfetti { .. }
            | SlashOutcome::OverlayDeferred(_)
            | SlashOutcome::Quit
            | SlashOutcome::OpenFeedbackDialog
            | SlashOutcome::OpenProviderConnectDialog { .. }
            | SlashOutcome::OpenProviderDisconnectDialog { .. }
            | SlashOutcome::OpenProviderListDialog { .. }
            | SlashOutcome::OpenMemoryResultDialog { .. }
    )
}

/// Whether the slash outcome should show an agent turn indicator.
pub fn slash_echoes_prompt_in_transcript(outcome: &SlashOutcome) -> bool {
    matches!(outcome, SlashOutcome::SpawnAgentTurn | SlashOutcome::BackgroundTask)
}

pub fn overlay_deferred_message(overlay: &OverlayCommand) -> String {
    match overlay {
        OverlayCommand::Model { .. } => "/model overlay not yet implemented".into(),
        OverlayCommand::ScopedModels => "/scoped-models overlay not yet implemented".into(),
        OverlayCommand::Tree => "/tree overlay not yet implemented".into(),
        OverlayCommand::Resume => "/resume overlay not yet implemented".into(),
        OverlayCommand::ProviderConnect { .. } => "/provider connect overlay not yet implemented".into(),
    }
}

pub fn provider_list_slash_message() -> String {
    use crate::tui::provider_connect_dialog::{ProviderConfigStatus, get_provider_options};

    let providers = get_provider_options();
    let mut configured: Vec<_> = providers
        .iter()
        .filter(|p| !matches!(p.config_status, ProviderConfigStatus::Unconfigured))
        .collect();
    configured.sort_by_key(|p| &p.id);

    let mut lines = vec![format!("{} configured provider(s):\n", configured.len())];

    if configured.is_empty() {
        lines.push("  (none)".into());
        lines.push(String::new());
        lines.push(
            "Tip: Use /provider connect or `elph provider connect <id> --env <VAR>` to register a provider.".into(),
        );
        return lines.join("\n");
    }

    let max_id = configured.iter().map(|p| p.id.len()).max().unwrap_or(0);
    for p in configured {
        let status = match &p.config_status {
            ProviderConfigStatus::ApiKeyConfigured => "API key".into(),
            ProviderConfigStatus::OAuthConfigured => "OAuth".into(),
            ProviderConfigStatus::EnvVarConfigured(var) => format!("env: {var}"),
            ProviderConfigStatus::Unconfigured => unreachable!(),
        };
        lines.push(format!("  {:<max_id$}  {}", p.id, status, max_id = max_id));
    }

    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bare_model_slash_opens_selector() {
        let outcome = handle_slash_submit(SlashContext {
            input: "/model",
            extensions: None,
            prompt_templates: None,
            skills: None,
            agent_session: None,
            extension_host: None,
            paths: None,
            cwd: None,
            spawn_agent_work: true,
        });
        assert!(matches!(
            outcome,
            SlashOutcome::OpenModelSelector { filter } if filter.is_empty()
        ));
    }

    #[test]
    fn scoped_models_slash_opens_editor() {
        let outcome = handle_slash_submit(SlashContext {
            input: "/scoped-models",
            extensions: None,
            prompt_templates: None,
            skills: None,
            agent_session: None,
            extension_host: None,
            paths: None,
            cwd: None,
            spawn_agent_work: true,
        });
        assert!(matches!(outcome, SlashOutcome::OpenScopedModels));
    }

    #[test]
    fn model_slash_opens_selector() {
        let outcome = handle_slash_submit(SlashContext {
            input: "/model opus",
            extensions: None,
            prompt_templates: None,
            skills: None,
            agent_session: None,
            extension_host: None,
            paths: None,
            cwd: None,
            spawn_agent_work: true,
        });
        assert!(matches!(
            outcome,
            SlashOutcome::OpenModelSelector { filter } if filter == "opus"
        ));
    }

    #[test]
    fn local_slash_outcomes_skip_prompt_echo() {
        assert!(!slash_echoes_prompt_in_transcript(&SlashOutcome::Assistant(String::new())));
        assert!(!slash_echoes_prompt_in_transcript(&SlashOutcome::Status(String::new())));
        assert!(!slash_echoes_prompt_in_transcript(&SlashOutcome::OpenModelSelector {
            filter: String::new()
        }));
        assert!(!slash_echoes_prompt_in_transcript(&SlashOutcome::OpenSystemPromptDialog {
            text: String::new()
        }));
        assert!(!slash_echoes_prompt_in_transcript(&SlashOutcome::PlayConfetti {
            mode: crate::tui::confetti::ConfettiMode::Confetti
        }));
        assert!(slash_echoes_prompt_in_transcript(&SlashOutcome::SpawnAgentTurn));
    }

    #[test]
    fn tools_json_is_rejected_without_session() {
        let outcome = handle_slash_submit(SlashContext {
            input: "/tools json",
            extensions: None,
            prompt_templates: None,
            skills: None,
            agent_session: None,
            extension_host: None,
            paths: None,
            cwd: None,
            spawn_agent_work: true,
        });
        assert!(matches!(
            outcome,
            SlashOutcome::Status(message) if message.contains("unknown /tools format")
        ));
    }

    #[test]
    fn tools_unknown_format_returns_status() {
        let outcome = handle_slash_submit(SlashContext {
            input: "/tools yaml",
            extensions: None,
            prompt_templates: None,
            skills: None,
            agent_session: None,
            extension_host: None,
            paths: None,
            cwd: None,
            spawn_agent_work: true,
        });
        assert!(matches!(
            outcome,
            SlashOutcome::Status(message) if message.contains("unknown /tools format")
        ));
    }

    #[test]
    fn tools_returns_assistant_markdown_without_session() {
        let outcome = handle_slash_submit(SlashContext {
            input: "/tools",
            extensions: None,
            prompt_templates: None,
            skills: None,
            agent_session: None,
            extension_host: None,
            paths: None,
            cwd: None,
            spawn_agent_work: true,
        });
        assert!(!slash_echoes_prompt_in_transcript(&outcome));
        assert!(matches!(
            outcome,
            SlashOutcome::Assistant(message)
                if message.contains("## Available tools")
                    && message.contains("| Tool | Group | Description |")
                    && message.contains("Agent session unavailable")
        ));
    }

    #[test]
    fn system_prompt_without_session_returns_status() {
        let outcome = handle_slash_submit(SlashContext {
            input: "/system-prompt",
            extensions: None,
            prompt_templates: None,
            skills: None,
            agent_session: None,
            extension_host: None,
            paths: None,
            cwd: None,
            spawn_agent_work: true,
        });
        assert!(matches!(
            outcome,
            SlashOutcome::Status(ref message) if message == "Agent session required for this command."
        ));
        assert!(slash_outcome_is_ui_only(&outcome));
    }

    #[test]
    fn session_without_session_returns_status() {
        let outcome = handle_slash_submit(SlashContext {
            input: "/session",
            extensions: None,
            prompt_templates: None,
            skills: None,
            agent_session: None,
            extension_host: None,
            paths: None,
            cwd: None,
            spawn_agent_work: true,
        });
        assert!(matches!(
            outcome,
            SlashOutcome::Status(ref message) if message == "Agent session required for this command."
        ));
        assert!(slash_outcome_is_ui_only(&outcome));
    }

    #[test]
    fn rename_without_session_returns_status() {
        let outcome = handle_slash_submit(SlashContext {
            input: "/rename",
            extensions: None,
            prompt_templates: None,
            skills: None,
            agent_session: None,
            extension_host: None,
            paths: None,
            cwd: None,
            spawn_agent_work: true,
        });
        assert!(matches!(
            outcome,
            SlashOutcome::Status(ref message) if message == "Agent session required for this command."
        ));
        assert!(slash_outcome_is_ui_only(&outcome));
    }

    #[test]
    fn ui_only_outcomes_do_not_include_spawn_turn() {
        assert!(slash_outcome_is_ui_only(&SlashOutcome::OpenSystemPromptDialog {
            text: "x".into()
        }));
        assert!(slash_outcome_is_ui_only(&SlashOutcome::Assistant("tools".into())));
        assert!(!slash_outcome_is_ui_only(&SlashOutcome::SpawnAgentTurn));
    }

    #[test]
    fn confetti_opens_rain_overlay() {
        let outcome = handle_slash_submit(SlashContext {
            input: "/confetti",
            extensions: None,
            prompt_templates: None,
            skills: None,
            agent_session: None,
            extension_host: None,
            paths: None,
            cwd: None,
            spawn_agent_work: true,
        });
        assert!(matches!(
            outcome,
            SlashOutcome::PlayConfetti { mode } if mode == crate::tui::confetti::ConfettiMode::Confetti
        ));
    }

    #[test]
    fn confetti_firework_mode() {
        let outcome = handle_slash_submit(SlashContext {
            input: "/confetti firework",
            extensions: None,
            prompt_templates: None,
            skills: None,
            agent_session: None,
            extension_host: None,
            paths: None,
            cwd: None,
            spawn_agent_work: true,
        });
        assert!(matches!(
            outcome,
            SlashOutcome::PlayConfetti { mode } if mode == crate::tui::confetti::ConfettiMode::Firework
        ));
    }

    #[test]
    fn help_returns_status_without_session() {
        let outcome = handle_slash_submit(SlashContext {
            input: "/help",
            extensions: None,
            prompt_templates: None,
            skills: None,
            agent_session: None,
            extension_host: None,
            paths: None,
            cwd: None,
            spawn_agent_work: true,
        });
        assert!(matches!(outcome, SlashOutcome::Status(message) if message.contains("Slash commands:")));
    }

    #[test]
    fn unknown_slash_is_unimplemented() {
        let outcome = handle_slash_submit(SlashContext {
            input: "/not-a-real-command",
            extensions: None,
            prompt_templates: None,
            skills: None,
            agent_session: None,
            extension_host: None,
            paths: None,
            cwd: None,
            spawn_agent_work: true,
        });
        assert!(matches!(
            outcome,
            SlashOutcome::Unimplemented(message) if message == "not-a-real-command not yet implemented"
        ));
    }

    #[test]
    fn skill_slash_without_session_returns_status() {
        let skill = elph_agent::Skill {
            name: "debug".into(),
            description: "Debug".into(),
            content: "Steps".into(),
            file_path: "/tmp/debug/SKILL.md".into(),
            ..Default::default()
        };
        let outcome = handle_slash_submit(SlashContext {
            input: "/skill:debug src/main.rs",
            extensions: None,
            prompt_templates: None,
            skills: Some(&[skill]),
            agent_session: None,
            extension_host: None,
            paths: None,
            cwd: None,
            spawn_agent_work: true,
        });
        assert!(matches!(
            outcome,
            SlashOutcome::Status(message) if message == "Agent session required for this command."
        ));
    }

    #[test]
    fn skill_slash_missing_required_args_returns_notice() {
        let skill = elph_agent::Skill {
            name: "code-review".into(),
            description: "Review".into(),
            content: "Review".into(),
            file_path: "/tmp/code-review/SKILL.md".into(),
            argument_hint: Some("<file-path>".into()),
            ..Default::default()
        };
        let outcome = handle_slash_submit(SlashContext {
            input: "/code-review",
            extensions: None,
            prompt_templates: None,
            skills: Some(&[skill]),
            agent_session: None,
            extension_host: None,
            paths: None,
            cwd: None,
            spawn_agent_work: true,
        });
        assert!(matches!(
            outcome,
            SlashOutcome::Status(message)
                if message.contains("requires arguments")
                    && message.contains("code-review")
                    && message.contains("<file-path>")
        ));
    }
}
