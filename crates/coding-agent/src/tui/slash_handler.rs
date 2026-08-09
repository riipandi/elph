//! Slash command outcomes for the TUI shell.

use std::path::Path;
use std::sync::Arc;

use elph_agent::{ExtensionRegistry, PromptTemplate, Skill};

use crate::agent::RETRY_CONTINUE_PROMPT;
use crate::agent::{OverlayCommand, SlashDispatch};
use crate::agent::{
    confetti_mode_from_args, dispatch_slash_command, format_help_message, session_info_slash_message,
    session_title_for_rename, slash_unimplemented_message, system_prompt_slash_message, tools_slash_message,
};
use crate::extensions::ExtensionHost;
use crate::platform::Paths;
use crate::tui::confetti::confetti_mode_from_slash_args;
use crate::utils::path::AppPaths;

use super::agent_bridge::{SlashDispatcher, TurnDispatcher};

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
    Unimplemented(String),
    SpawnAgentTurn,
    /// Like [`SlashOutcome::SpawnAgentTurn`], but the slash input is NOT echoed as a
    /// user prompt card (e.g. `/compact` — the compaction notice already communicates it).
    SpawnAgentTurnQuiet,
    OverlayDeferred(OverlayCommand),
    OpenModelSelector {
        filter: String,
    },
    OpenScopedModels,
    OpenSystemPromptDialog {
        text: String,
    },
    /// Active tools viewer (ScrollTextDialog, like `/system-prompt`).
    OpenToolsDialog {
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
    /// Provider catalog update result viewer (ScrollTextDialog).
    OpenProviderUpdateDialog {
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
    /// Open feedback dialog (Report a Bug / Join Community / Support).
    OpenFeedbackDialog,
    /// Open provider connection dialog with OAuth or API key input.
    OpenProviderConnectDialog {
        provider_id: Option<String>,
    },
    /// Open provider disconnect dialog to remove stored credentials.
    OpenProviderDisconnectDialog {
        provider_id: Option<String>,
    },
    /// Open MCP OAuth dialog (`/mcp auth [name]`).
    OpenMcpAuthDialog {
        server_name: Option<String>,
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

/// Handle `/handover` slash commands.
///
/// Syntax: `/handover <tool> [ref]` where `<tool>` is `claude` or `codex`.
///
/// - `claude`: resolves the referenced Claude Code session for the current cwd,
///   reads it as inert history, and injects a handoff prompt into the current
///   agent session (a background turn — no `/handover` user card is echoed).
/// - `codex`: same flow against Codex rollout transcripts (`~/.codex/sessions`).
fn handle_handover_slash(ctx: SlashContext<'_>, args: &str) -> SlashOutcome {
    let mut parts = args.splitn(2, char::is_whitespace);
    let tool = parts.next().unwrap_or("").trim().to_ascii_lowercase();
    let reference = parts.next().unwrap_or("").trim().to_string();

    match tool.as_str() {
        "" => SlashOutcome::Status(
            "Usage: /handover <claude|codex> [latest|<session-id>|<free-text>]\n\
             Example: /handover claude latest"
                .into(),
        ),
        "claude" => handle_claude_handover(ctx, &reference),
        "codex" => handle_codex_handover(ctx, &reference),
        other => SlashOutcome::Status(format!(
            "Unknown handover tool `{other}` — use `/handover claude` or `/handover codex`."
        )),
    }
}

/// `/handover claude [ref]` — Claude Code session resume.
fn handle_claude_handover(ctx: SlashContext<'_>, reference: &str) -> SlashOutcome {
    use crate::agent::{
        HandoverError, build_handoff_prompt, claude_config_dir, read_claude_session, resolve_claude_session,
    };

    let Some(agent_session) = ctx.agent_session.as_ref() else {
        return SlashOutcome::Status("Agent session required for this command.".into());
    };
    let Some(cwd) = ctx.cwd else {
        return SlashOutcome::Status("Working directory required for /handover claude.".into());
    };

    let config_dir = match claude_config_dir() {
        Some(dir) => dir,
        None => {
            return SlashOutcome::Status("Could not locate Claude config directory (expected ~/.claude).".to_string());
        }
    };

    let reference_opt = if reference.is_empty() { None } else { Some(reference) };
    match resolve_claude_session(cwd, Some(&config_dir), reference_opt) {
        Ok(session) => match read_claude_session(&session.path) {
            Ok(handover) => {
                if ctx.spawn_agent_work {
                    let prompt = build_handoff_prompt(&handover, 0);
                    let session = agent_session.clone();
                    TurnDispatcher::spawn_turn(session, prompt, false);
                }
                // Quiet: the injected handoff prompt is the turn; do NOT echo a
                // "/handover claude" user card (the handoff text carries the context).
                SlashOutcome::SpawnAgentTurnQuiet
            }
            Err(HandoverError::ReadFailed(message)) => {
                SlashOutcome::Status(format!("Failed to read Claude session: {message}"))
            }
            Err(err) => SlashOutcome::Status(err.to_string()),
        },
        Err(HandoverError::Ambiguous { matches, .. }) => ambiguous_session_status("Claude", matches),
        Err(HandoverError::NoSession(message)) => SlashOutcome::Status(message),
        Err(err) => SlashOutcome::Status(err.to_string()),
    }
}

/// `/handover codex [ref]` — Codex session resume.
fn handle_codex_handover(ctx: SlashContext<'_>, reference: &str) -> SlashOutcome {
    use crate::agent::{
        HandoverError, build_codex_handoff_prompt, codex_config_dir, read_codex_session, resolve_codex_session,
    };

    let Some(agent_session) = ctx.agent_session.as_ref() else {
        return SlashOutcome::Status("Agent session required for this command.".into());
    };
    let Some(cwd) = ctx.cwd else {
        return SlashOutcome::Status("Working directory required for /handover codex.".into());
    };

    let config_dir = match codex_config_dir() {
        Some(dir) => dir,
        None => {
            return SlashOutcome::Status("Could not locate Codex config directory (expected ~/.codex).".to_string());
        }
    };

    let reference_opt = if reference.is_empty() { None } else { Some(reference) };
    match resolve_codex_session(cwd, Some(&config_dir), reference_opt) {
        Ok(session) => match read_codex_session(&session.path) {
            Ok(handover) => {
                if ctx.spawn_agent_work {
                    let prompt = build_codex_handoff_prompt(&handover, 0);
                    let session = agent_session.clone();
                    TurnDispatcher::spawn_turn(session, prompt, false);
                }
                SlashOutcome::SpawnAgentTurnQuiet
            }
            Err(HandoverError::ReadFailed(message)) => {
                SlashOutcome::Status(format!("Failed to read Codex session: {message}"))
            }
            Err(err) => SlashOutcome::Status(err.to_string()),
        },
        Err(HandoverError::Ambiguous { matches, .. }) => ambiguous_session_status("Codex", matches),
        Err(HandoverError::NoSession(message)) => SlashOutcome::Status(message),
        Err(err) => SlashOutcome::Status(err.to_string()),
    }
}

/// Format an ambiguous free-text reference: list candidate ids so the user can
/// resume one by native id.
fn ambiguous_session_status(tool: &str, matches: Vec<crate::agent::HandoverSession>) -> SlashOutcome {
    let mut lines = vec![format!(
        "Multiple {tool} sessions match, resume one by id (`/handover {} <id>`):",
        tool.to_ascii_lowercase()
    )];
    for session in matches {
        lines.push(format!("  {}  {}", session.session_id, session.title));
    }
    SlashOutcome::Status(lines.join("\n"))
}

pub fn handle_slash_submit(ctx: SlashContext<'_>) -> SlashOutcome {
    let Some(dispatch) = dispatch_slash_command(ctx.input, ctx.extensions, ctx.prompt_templates, ctx.skills) else {
        return SlashOutcome::SpawnAgentTurn;
    };

    // Memory commands run without an agent session — dispatch immediately.
    if let SlashDispatch::Memory { ref args } = dispatch {
        return handle_memory_slash(ctx, args);
    }

    // Handover commands read the foreign session store and inject a turn.
    if let SlashDispatch::Handover { ref args } = dispatch {
        return handle_handover_slash(ctx, args);
    }

    match dispatch {
        SlashDispatch::Quit => SlashOutcome::Quit,
        SlashDispatch::NewSession => SlashOutcome::NewSession,
        SlashDispatch::Help => {
            SlashOutcome::Status(format_help_message(ctx.extensions, ctx.prompt_templates, ctx.skills))
        }
        SlashDispatch::Tools { .. } => match tools_slash_message(ctx.agent_session.as_ref()) {
            Ok(text) => SlashOutcome::OpenToolsDialog { text },
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
        SlashDispatch::ProviderUpdate { provider_id } => {
            let Some(paths) = ctx.paths else {
                return SlashOutcome::Status("Paths required for /provider update.".into());
            };
            let dir = paths.providers_dir();
            let providers: Vec<String> = if let Some(pid) = provider_id {
                if !elph_ai::embedded_provider_ids().contains(&pid.as_str()) {
                    return SlashOutcome::Status(format!("Unknown builtin provider: {pid}"));
                }
                vec![pid.clone()]
            } else {
                elph_ai::embedded_provider_ids().iter().map(|s| s.to_string()).collect()
            };
            match run_provider_update(&dir, &providers) {
                Ok(text) => SlashOutcome::OpenProviderUpdateDialog { text },
                Err(e) => SlashOutcome::Status(format!("Provider update failed: {e}")),
            }
        }
        SlashDispatch::McpAuth { server_name } => SlashOutcome::OpenMcpAuthDialog { server_name },
        SlashDispatch::McpLogout { server_name } => {
            let Some(paths) = ctx.paths else {
                return SlashOutcome::Status("Paths required for /mcp logout.".into());
            };
            match server_name {
                Some(name) => match crate::tui::mcp_auth_dialog::logout_mcp_server(paths, &name) {
                    Ok(msg) => SlashOutcome::Status(msg),
                    Err(msg) => SlashOutcome::Status(msg),
                },
                None => SlashOutcome::Status(
                    "Usage: /mcp logout <server> — e.g. /mcp logout figma\nList servers: /mcp list".into(),
                ),
            }
        }
        SlashDispatch::McpList => {
            let Some(paths) = ctx.paths else {
                return SlashOutcome::Status("Paths required for /mcp list.".into());
            };
            SlashOutcome::OpenProviderListDialog {
                text: crate::tui::mcp_auth_dialog::mcp_list_slash_message(paths),
            }
        }
        // Handled by early return above — unreachable here.
        SlashDispatch::Memory { .. } => unreachable!(),
        SlashDispatch::Handover { .. } => unreachable!(),
        SlashDispatch::Unimplemented(command) => SlashOutcome::Unimplemented(slash_unimplemented_message(&command)),
        SlashDispatch::OverlayNeeded(overlay) => match overlay {
            OverlayCommand::ProviderConnect { .. } => SlashOutcome::OverlayDeferred(overlay),
            OverlayCommand::Model { filter } => SlashOutcome::OpenModelSelector { filter },
            OverlayCommand::ScopedModels => SlashOutcome::OpenScopedModels,
            other => SlashOutcome::OverlayDeferred(other),
        },
        SlashDispatch::Continue => {
            if ctx.agent_session.is_none() {
                return SlashOutcome::Status("Agent session required for this command.".into());
            }
            if ctx.spawn_agent_work {
                // Submit the recovery prompt (not "/continue") so the model resumes the
                // interrupted task without re-doing completed tool work. The tick loop
                // renders the matching UserPromptCommitted as a "Continuing tasks…" meta
                // line — via SpawnAgentTurnQuiet no "/continue" user card is echoed.
                let session = ctx.agent_session.clone().expect("checked above");
                TurnDispatcher::spawn_turn(session, RETRY_CONTINUE_PROMPT.to_string(), false);
            }
            SlashOutcome::SpawnAgentTurnQuiet
        }
        SlashDispatch::Compact { .. } | SlashDispatch::PromptTemplate { .. } => {
            let is_compact = matches!(dispatch, SlashDispatch::Compact { .. });
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
            // `/compact` must not echo a "/compact" user prompt card — the compaction
            // notice already communicates it. Other turn-spawning slash commands do echo.
            if is_compact {
                SlashOutcome::SpawnAgentTurnQuiet
            } else {
                SlashOutcome::SpawnAgentTurn
            }
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
            | SlashOutcome::Unimplemented(_)
            | SlashOutcome::NewSession
            | SlashOutcome::BackgroundTask
            | SlashOutcome::OpenModelSelector { .. }
            | SlashOutcome::OpenScopedModels
            | SlashOutcome::OpenSystemPromptDialog { .. }
            | SlashOutcome::OpenToolsDialog { .. }
            | SlashOutcome::OpenSessionInfoDialog { .. }
            | SlashOutcome::OpenRenameDialog { .. }
            | SlashOutcome::PlayConfetti { .. }
            | SlashOutcome::OverlayDeferred(_)
            | SlashOutcome::Quit
            | SlashOutcome::OpenFeedbackDialog
            | SlashOutcome::OpenProviderConnectDialog { .. }
            | SlashOutcome::OpenProviderDisconnectDialog { .. }
            | SlashOutcome::OpenProviderListDialog { .. }
            | SlashOutcome::OpenProviderUpdateDialog { .. }
            | SlashOutcome::OpenMcpAuthDialog { .. }
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

/// Plan, apply a non-destructive merge, and produce a result summary for
/// `/provider update`. Merge keeps the user's on-disk file and only adds seed
/// models that are missing, so custom configuration is never overwritten. After
/// writing, the in-memory catalog cache is invalidated so the running session
/// picks up the new models.
fn run_provider_update(dir: &Path, providers: &[String]) -> Result<String, String> {
    let plan = elph_ai::plan_provider_update(dir, providers)?;
    if plan.entries.is_empty() {
        return Ok("No builtin provider catalogs to update.".to_string());
    }

    let resolved = |e: &elph_ai::ProviderUpdatePlanEntry| -> elph_ai::UpdatePolicy {
        // TUI applies the safe, non-destructive default: merge (keep custom
        // config). Unparsable files are left untouched rather than clobbered.
        if e.unparsable {
            elph_ai::UpdatePolicy::SkipExisting
        } else {
            elph_ai::UpdatePolicy::Merge
        }
    };
    let report = elph_ai::apply_provider_update(dir, &plan, resolved)?;

    // Drop the in-memory catalog cache so the next model lookup re-reads disk.
    elph_ai::invalidate_catalog_cache();

    let mut lines: Vec<String> = vec![
        "Provider catalogs updated (your custom config is preserved).".to_string(),
        String::new(),
    ];
    for e in &plan.entries {
        match e.status {
            elph_ai::ProviderUpdateStatus::UpToDate => continue,
            _ => {
                let verb = if matches!(e.status, elph_ai::ProviderUpdateStatus::New) {
                    "written"
                } else {
                    "merged"
                };
                let mut detail = Vec::new();
                if !e.added.is_empty() {
                    detail.push(format!("+{} new", e.added.len()));
                }
                if !e.changed.is_empty() {
                    detail.push(format!("~{} kept custom", e.changed.len()));
                }
                let suffix = if detail.is_empty() {
                    String::new()
                } else {
                    format!(" ({})", detail.join(", "))
                };
                lines.push(format!("  {} — {}{}", e.provider, verb, suffix));
            }
        }
    }
    lines.push(String::new());
    lines.push(format!(
        "{} written · {} merged · {} skipped · {} up to date",
        report.written, report.merged, report.skipped, report.up_to_date
    ));
    Ok(lines.join("\n"))
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
        assert!(!slash_echoes_prompt_in_transcript(&SlashOutcome::OpenToolsDialog {
            text: String::new()
        }));
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
        // `/compact` spawns a turn but must NOT echo a "/compact" user prompt card.
        assert!(!slash_echoes_prompt_in_transcript(&SlashOutcome::SpawnAgentTurnQuiet));
    }

    #[test]
    fn tools_junk_arg_returns_dialog_without_session() {
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
        assert!(!slash_echoes_prompt_in_transcript(&outcome));
        assert!(matches!(
            outcome,
            SlashOutcome::OpenToolsDialog { text }
                if text.contains("Available tools")
                    && text.contains("session unavailable")
                    && !text.contains("| Tool |")
        ));
    }

    #[test]
    fn tools_returns_dialog_without_session() {
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
            SlashOutcome::OpenToolsDialog { text }
                if text.contains("Available tools")
                    && text.contains("session unavailable")
                    && !text.contains("| Tool |")
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
    fn continue_without_session_returns_status() {
        let outcome = handle_slash_submit(SlashContext {
            input: "/continue",
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
        assert!(slash_outcome_is_ui_only(&SlashOutcome::OpenToolsDialog {
            text: "tools".into()
        }));
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

    fn temp_paths() -> Paths {
        let tmp = tempfile::tempdir().expect("tempdir");
        Paths::from_dirs(tmp.path().join("config"), tmp.path().join("data"), tmp.path().join("project"))
    }

    #[test]
    fn provider_update_writes_catalog_and_returns_dialog() {
        let paths = temp_paths();
        let outcome = handle_slash_submit(SlashContext {
            input: "/provider update anthropic",
            extensions: None,
            prompt_templates: None,
            skills: None,
            agent_session: None,
            extension_host: None,
            paths: Some(&paths),
            cwd: None,
            spawn_agent_work: true,
        });
        let text = match outcome {
            SlashOutcome::OpenProviderUpdateDialog { ref text } => text.clone(),
            other => panic!("expected OpenProviderUpdateDialog, got {other:?}"),
        };
        assert!(text.contains("Provider catalogs updated"), "text: {text}");
        assert!(
            paths.providers_dir().join("anthropic.json").is_file(),
            "catalog file should be written"
        );
        // The result must be a UI-only outcome (no spawned turn / no prompt echo).
        assert!(slash_outcome_is_ui_only(&outcome));
    }

    #[test]
    fn provider_update_unknown_provider_returns_status() {
        let paths = temp_paths();
        let outcome = handle_slash_submit(SlashContext {
            input: "/provider update not-a-real-provider",
            extensions: None,
            prompt_templates: None,
            skills: None,
            agent_session: None,
            extension_host: None,
            paths: Some(&paths),
            cwd: None,
            spawn_agent_work: true,
        });
        assert!(matches!(
            outcome,
            SlashOutcome::Status(ref message) if message.contains("Unknown builtin provider")
        ));
    }

    #[test]
    fn handover_codex_without_session_returns_status() {
        let outcome = handle_slash_submit(SlashContext {
            input: "/handover codex",
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
    fn handover_missing_tool_shows_usage() {
        let outcome = handle_slash_submit(SlashContext {
            input: "/handover",
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
            SlashOutcome::Status(ref message) if message.contains("Usage: /handover")
        ));
    }

    #[test]
    fn handover_unknown_tool_shows_usage() {
        let outcome = handle_slash_submit(SlashContext {
            input: "/handover cursor",
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
            SlashOutcome::Status(ref message) if message.contains("Unknown handover tool `cursor`")
        ));
    }

    #[test]
    fn handover_outcome_is_ui_only_for_codex() {
        assert!(slash_outcome_is_ui_only(&SlashOutcome::Status(
            "Agent session required for this command.".to_string()
        )));
    }
}
