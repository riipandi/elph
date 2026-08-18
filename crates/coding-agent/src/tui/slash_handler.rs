//! Slash command outcomes for the TUI shell.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use elph_agent::harness::{PromptTemplate, Skill};
use elph_agent::plugins::ExtensionRegistry;

use crate::agent::RETRY_CONTINUE_PROMPT;
use crate::agent::{
    HOTKEYS_TEXT, changelog_text, clone_session_message, confetti_mode_from_args, dispatch_slash_command,
    export_session_message, fork_session_message, format_help_message, import_session_from_jsonl, import_slash_message,
    session_info_slash_message, session_title_for_rename, settings_slash_message, slash_unimplemented_message,
    system_prompt_slash_message, tools_slash_message, tree_slash_message, trust_slash_message, workers_slash_message,
};
use crate::agent::{OverlayCommand, SlashDispatch, TransferError, TransferSession, spawn_aside};
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
        let (memory_count, task_count) = match elph_agent::runtime::try_block_on(crate::memory::flush_preview(paths)) {
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
    match elph_agent::runtime::try_block_on(crate::memory::slash_run(&paths, &args)) {
        Ok(Ok(text)) => SlashOutcome::OpenMemoryResultDialog { text },
        Ok(Err(err)) => SlashOutcome::Status(format!("Memory error: {err}")),
        Err(err) => SlashOutcome::Status(format!("Memory error: {err:#}")),
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum SlashOutcome {
    Quit,
    NewSession,
    BackgroundTask,
    Status(String),
    Unimplemented(String),
    SpawnAgentTurn,
    /// Spawn agent turn from a skill (transcript echoes `/skill:name`).
    SpawnAgentTurnSkill {
        name: String,
    },
    /// Spawn agent turn from a prompt template (echoed as `/name` in transcript).
    SpawnAgentTurnPromptTemplate {
        name: String,
    },
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
    OpenViewPlanDialog {
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
    /// A background `/transfer` task was dispatched directly. Unlike
    /// [`SlashOutcome::BackgroundTask`], the work's final user-visible text is
    /// delivered via its own transcript events (slim transfer meta line) rather
    /// than echoing the raw slash input as a user card. The shell treats this as
    /// a no-op; the task drives busy UI through normal stream events, so a read
    /// failure never leaves the host stuck "busy".
    BackgroundTaskQuiet,
    /// Reload TUI bootstrap against another session id (`/resume <id>`).
    ResumeSession {
        session_id: String,
    },
    /// Interactive list picker (`/resume`, `/tree` without a direct id).
    OpenItemSelector {
        purpose: crate::tui::item_selector::ItemSelectorPurpose,
        title: String,
        items: Vec<crate::types::SelectItem>,
        preferred_value: Option<String>,
        footer_hint: String,
    },
    /// Open the worker chat overlay (`/intercom`). `peers` seeds the picker; the
    /// shell loads inbox history from the live session on the same open path as Alt+M.
    OpenWorkerChat {
        peers: Vec<elph_agent::workers::LiveWorker>,
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
}

/// Handle `/transfer` slash commands.
///
/// Syntax: `/transfer <tool> [ref]` where `<tool>` is `claude` or `codex`.
///
/// Both tools resolve the referenced session for the current cwd, read it as
/// inert history, and inject a handoff prompt into the current agent session.
/// The heavy file I/O + parse runs on a **background task** (never the TUI
/// thread), so even a large transcript cannot block the render loop. Errors are
/// surfaced via a status notice; nothing is echoed as a `/transfer` user card.
fn handle_transfer_slash(ctx: SlashContext<'_>, args: &str) -> SlashOutcome {
    let mut parts = args.splitn(2, char::is_whitespace);
    let tool = parts.next().unwrap_or("").trim().to_ascii_lowercase();
    let reference = parts.next().unwrap_or("").trim().to_string();

    let tool_kind = match tool.as_str() {
        "" => {
            return SlashOutcome::Status(
                "Usage: /transfer <claude|codex> [latest|<session-id>|<free-text>]\n\
                 Example: /transfer claude latest"
                    .into(),
            );
        }
        "claude" => TransferTool::Claude,
        "codex" => TransferTool::Codex,
        other => {
            return SlashOutcome::Status(format!(
                "Unknown transfer tool `{other}` — use `/transfer claude` or `/transfer codex`."
            ));
        }
    };

    let Some(agent_session) = ctx.agent_session.as_ref() else {
        return SlashOutcome::Status("Agent session required for this command.".into());
    };
    let Some(cwd) = ctx.cwd else {
        return SlashOutcome::Status(format!("Working directory required for /transfer {}.", tool_kind.name()));
    };

    let reference_opt = if reference.is_empty() {
        None
    } else {
        Some(reference.clone())
    };
    let session = agent_session.clone();
    let cwd = cwd.to_path_buf();
    tokio::spawn(async move {
        // Resolve + read + prompt-build run on a blocking pool thread, off the
        // TUI render loop. `spawn_blocking` is a no-op dispatch when we are
        // already inside a blocking context, which is fine here (the caller is
        // never inside one).
        let outcome =
            tokio::task::spawn_blocking(move || run_transfer_resolution(tool_kind, &cwd, reference_opt.as_deref()))
                .await;
        match outcome {
            Ok(Ok(prompt)) => {
                if let Err(err) = session.submit_prompt(prompt, false).await {
                    log::warn!("transfer turn failed: {err}");
                }
            }
            Ok(Err(message)) => emit_transfer_status(&session, message),
            Err(join_err) => emit_transfer_status(&session, format!("Transfer task failed: {join_err}")),
        }
    });
    // Quiet background dispatch: no slash card echo; busy UI is driven by the
    // agent loop's own stream events (RunCompleted clears it), so a read failure
    // never strands a stale "busy" chip.
    SlashOutcome::BackgroundTaskQuiet
}

/// Which foreign tool a `/transfer` refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TransferTool {
    Claude,
    Codex,
}

impl TransferTool {
    fn name(self) -> &'static str {
        match self {
            TransferTool::Claude => "claude",
            TransferTool::Codex => "codex",
        }
    }

    fn display(self) -> &'static str {
        match self {
            TransferTool::Claude => "Claude Code",
            TransferTool::Codex => "Codex",
        }
    }

    fn config_dir(self) -> Option<PathBuf> {
        match self {
            TransferTool::Claude => crate::agent::claude_config_dir(),
            TransferTool::Codex => crate::agent::codex_config_dir(),
        }
    }

    fn resolve(self, cwd: &Path, config_dir: &Path, reference: Option<&str>) -> Result<TransferSession, TransferError> {
        match self {
            TransferTool::Claude => crate::agent::resolve_claude_session(cwd, Some(config_dir), reference),
            TransferTool::Codex => crate::agent::resolve_codex_session(cwd, Some(config_dir), reference),
        }
    }

    fn read(self, path: &Path) -> Result<TransferPayload, TransferError> {
        match self {
            TransferTool::Claude => crate::agent::read_claude_session(path).map(TransferPayload::Claude),
            TransferTool::Codex => crate::agent::read_codex_session(path).map(TransferPayload::Codex),
        }
    }

    fn build_prompt(self, payload: &TransferPayload) -> String {
        match (self, payload) {
            (TransferTool::Claude, TransferPayload::Claude(h)) => crate::agent::build_handoff_prompt(h, 0),
            (TransferTool::Codex, TransferPayload::Codex(h)) => crate::agent::build_codex_handoff_prompt(h, 0),
            _ => unreachable!("tool/payload mismatch"),
        }
    }

    fn config_dir_label(self) -> &'static str {
        match self {
            TransferTool::Claude => "~/.claude",
            TransferTool::Codex => "~/.codex",
        }
    }
}

/// A resolved + read + prompt-built transfer payload, ready to submit.
enum TransferPayload {
    Claude(crate::agent::ClaudeTransfer),
    Codex(crate::agent::CodexTransfer),
}

/// Run the (blocking) resolution + read + prompt-build. Returns the handoff
/// prompt, or a user-facing error message.
fn run_transfer_resolution(tool: TransferTool, cwd: &Path, reference: Option<&str>) -> Result<String, String> {
    let config_dir = tool.config_dir().ok_or_else(|| {
        format!(
            "Could not locate {} config directory (expected {}).",
            tool.display(),
            tool.config_dir_label()
        )
    })?;

    let session = match tool.resolve(cwd, &config_dir, reference) {
        Ok(session) => session,
        Err(TransferError::Ambiguous { matches, .. }) => {
            return Err(ambiguous_session_message(tool.display(), matches));
        }
        Err(err) => return Err(err.to_string()),
    };
    let payload = tool
        .read(&session.path)
        .map_err(|err| format!("Failed to read {} session: {err}", tool.display()))?;
    Ok(tool.build_prompt(&payload))
}

/// Format an ambiguous free-text reference: list candidate ids so the user can
/// resume one by native id.
fn ambiguous_session_message(tool: &str, matches: Vec<TransferSession>) -> String {
    let mut lines = vec![format!(
        "Multiple {tool} sessions match, resume one by id (`/transfer {tool_lower} <id>`):",
        tool_lower = tool.to_ascii_lowercase()
    )];
    for session in matches {
        lines.push(format!("  {}  {}", session.session_id, session.title));
    }
    lines.join("\n")
}

/// Push a status notice onto the host transcript (background task error path).
fn emit_transfer_status(session: &crate::agent::CodingAgentSession, message: String) {
    let _ = session
        .ui_event_sender()
        .send(crate::agent::AgentUiEvent::Status(message));
}

pub fn handle_slash_submit(ctx: SlashContext<'_>) -> SlashOutcome {
    let Some(dispatch) = dispatch_slash_command(ctx.input, ctx.extensions, ctx.prompt_templates, ctx.skills) else {
        return SlashOutcome::SpawnAgentTurn;
    };

    // Memory commands run without an agent session — dispatch immediately.
    if let SlashDispatch::Memory { ref args } = dispatch {
        return handle_memory_slash(ctx, args);
    }

    // Transfer commands read the foreign session store and inject a turn.
    if let SlashDispatch::Transfer { ref args } = dispatch {
        return handle_transfer_slash(ctx, args);
    }

    match dispatch {
        SlashDispatch::Quit => SlashOutcome::Quit,
        SlashDispatch::NewSession => SlashOutcome::NewSession,
        SlashDispatch::Aside { question } => {
            let question = question.trim().to_string();
            if question.is_empty() {
                return SlashOutcome::Status("Usage: /aside <question>".into());
            }
            let Some(session) = ctx.agent_session.clone() else {
                return SlashOutcome::Status("No active session for /aside".into());
            };
            let _request_id = spawn_aside(session, question);
            // Quiet: do not echo `/aside …` as a user prompt card.
            SlashOutcome::BackgroundTaskQuiet
        }
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
        SlashDispatch::ViewPlan => {
            let Some(paths) = ctx.paths else {
                return SlashOutcome::Status("No project directory for /view-plan".into());
            };
            let sid = ctx.agent_session.as_ref().map(|s| s.session_id().to_string());
            match crate::agent::plan_files::latest_plan_path(paths, sid.as_deref()) {
                Some(path) => match std::fs::read_to_string(&path) {
                    Ok(text) if !text.trim().is_empty() => SlashOutcome::OpenViewPlanDialog { text },
                    Ok(_) => SlashOutcome::Status("No plan written yet.".into()),
                    Err(err) => SlashOutcome::Status(format!("Could not read plan: {err}")),
                },
                None => SlashOutcome::Status("No plan written yet.".into()),
            }
        }
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
        SlashDispatch::Transfer { .. } => unreachable!(),
        SlashDispatch::Hotkeys => SlashOutcome::OpenSessionInfoDialog {
            text: HOTKEYS_TEXT.to_string(),
        },
        SlashDispatch::Changelog => SlashOutcome::OpenSessionInfoDialog { text: changelog_text() },
        SlashDispatch::Settings => {
            let Some(paths) = ctx.paths else {
                return SlashOutcome::Status("Paths required for /settings.".into());
            };
            SlashOutcome::OpenSessionInfoDialog {
                text: settings_slash_message(paths),
            }
        }
        SlashDispatch::Import { args } => {
            if args.trim().is_empty() {
                return SlashOutcome::OpenSessionInfoDialog {
                    text: import_slash_message(&args),
                };
            }
            let Some(session) = ctx.agent_session.as_ref() else {
                return SlashOutcome::Status("Agent session required for /import.".into());
            };
            let Some(cwd) = ctx.cwd else {
                return SlashOutcome::Status("Working directory required for /import.".into());
            };
            let session = Arc::clone(session);
            let cwd = cwd.to_path_buf();
            match elph_agent::runtime::try_block_on(import_session_from_jsonl(&session, &cwd, &args)) {
                Ok(Ok((_msg, new_id))) => SlashOutcome::ResumeSession { session_id: new_id },
                Ok(Err(message)) => SlashOutcome::Status(message),
                Err(e) => SlashOutcome::Status(format!("/import failed: {e}")),
            }
        }
        SlashDispatch::Trust => {
            let Some(paths) = ctx.paths else {
                return SlashOutcome::Status("Paths required for /trust.".into());
            };
            let Some(cwd) = ctx.cwd else {
                return SlashOutcome::Status("Working directory required for /trust.".into());
            };
            match trust_slash_message(paths, cwd) {
                Ok(text) => SlashOutcome::Status(text),
                Err(message) => SlashOutcome::Status(message),
            }
        }
        SlashDispatch::Workers => {
            let Some(session) = ctx.agent_session.as_ref() else {
                return SlashOutcome::Status("Agent session required for /workers.".into());
            };
            let session = Arc::clone(session);
            match elph_agent::runtime::try_block_on(workers_slash_message(Some(&session))) {
                Ok(Ok(text)) => SlashOutcome::OpenSessionInfoDialog { text },
                Ok(Err(message)) => SlashOutcome::Status(message),
                Err(e) => SlashOutcome::Status(format!("/workers failed: {e}")),
            }
        }
        SlashDispatch::WorkerChat => open_worker_chat_slash(ctx.agent_session.as_ref()),
        SlashDispatch::Tree { args } => {
            let Some(session) = ctx.agent_session.as_ref() else {
                return SlashOutcome::Status("Agent session required for /tree.".into());
            };
            let session = Arc::clone(session);
            let args = args.clone();
            let trimmed = args.trim();
            // Interactive picker when no target id (optional --branch filter).
            if trimmed.is_empty() || trimmed == "--branch" || trimmed == "branch" {
                let branch_only = trimmed == "--branch" || trimmed == "branch";
                return open_tree_item_selector(Some(&session), branch_only);
            }
            match elph_agent::runtime::try_block_on(tree_slash_message(&session, &args)) {
                Ok(Ok(_text)) => {
                    // Reload transcript so the TUI matches the new leaf (Pi chat re-render).
                    SlashOutcome::ResumeSession {
                        session_id: session.session_id().to_string(),
                    }
                }
                Ok(Err(message)) => SlashOutcome::Status(message),
                Err(e) => SlashOutcome::Status(format!("/tree failed: {e}")),
            }
        }
        SlashDispatch::Resume { args } => {
            let Some(session) = ctx.agent_session.as_ref() else {
                return SlashOutcome::Status("Agent session required for /resume.".into());
            };
            let id = args.trim();
            if !id.is_empty() {
                return SlashOutcome::ResumeSession {
                    session_id: id.to_string(),
                };
            }
            open_resume_item_selector(Some(session))
        }
        SlashDispatch::Export { args } => {
            let Some(session) = ctx.agent_session.as_ref() else {
                return SlashOutcome::Status("Agent session required for /export.".into());
            };
            let Some(cwd) = ctx.cwd else {
                return SlashOutcome::Status("Working directory required for /export.".into());
            };
            let session = Arc::clone(session);
            let cwd = cwd.to_path_buf();
            match elph_agent::runtime::try_block_on(export_session_message(&session, &cwd, &args)) {
                Ok(Ok(text)) => SlashOutcome::Status(text),
                Ok(Err(message)) => SlashOutcome::Status(message),
                Err(e) => SlashOutcome::Status(format!("/export failed: {e}")),
            }
        }
        SlashDispatch::Fork => {
            let Some(session) = ctx.agent_session.as_ref() else {
                return SlashOutcome::Status("Agent session required for /fork.".into());
            };
            let session = Arc::clone(session);
            match elph_agent::runtime::try_block_on(fork_session_message(&session)) {
                Ok(Ok(text)) => SlashOutcome::Status(text),
                Ok(Err(message)) => SlashOutcome::Status(message),
                Err(e) => SlashOutcome::Status(format!("/fork failed: {e}")),
            }
        }
        SlashDispatch::CloneSession => {
            let Some(session) = ctx.agent_session.as_ref() else {
                return SlashOutcome::Status("Agent session required for /clone.".into());
            };
            let session = Arc::clone(session);
            match elph_agent::runtime::try_block_on(clone_session_message(&session)) {
                Ok(Ok(text)) => SlashOutcome::Status(text),
                Ok(Err(message)) => SlashOutcome::Status(message),
                Err(e) => SlashOutcome::Status(format!("/clone failed: {e}")),
            }
        }
        SlashDispatch::Unimplemented(command) => SlashOutcome::Unimplemented(slash_unimplemented_message(&command)),
        SlashDispatch::OverlayNeeded(overlay) => match overlay {
            OverlayCommand::ProviderConnect { .. } => SlashOutcome::OverlayDeferred(overlay),
            OverlayCommand::Model { filter } => SlashOutcome::OpenModelSelector { filter },
            OverlayCommand::ScopedModels => SlashOutcome::OpenScopedModels,
            // Tree/Resume now have first-class dispatch; keep OverlayCommand for API compat.
            OverlayCommand::Tree => open_tree_item_selector(ctx.agent_session.as_ref(), false),
            OverlayCommand::Resume => open_resume_item_selector(ctx.agent_session.as_ref()),
        },
        SlashDispatch::Continue => {
            if ctx.agent_session.is_none() {
                return SlashOutcome::Status("Agent session required for this command.".into());
            }
            // Submit the recovery prompt (not "/continue") so the model resumes the
            // interrupted task without re-doing completed tool work. The tick loop
            // renders the matching UserPromptCommitted as a "Continuing tasks…" meta
            // line — via SpawnAgentTurnQuiet no "/continue" user card is echoed.
            //
            // Always dispatch (even while a turn is active): `run_prompt_turn` waits on
            // `turn_gate` and runs after the active turn completes. No raw "/continue"
            // text is queued to the model at the shell layer anymore.
            let session = ctx.agent_session.clone().expect("checked above");
            TurnDispatcher::spawn_turn(session, RETRY_CONTINUE_PROMPT.to_string(), false);
            SlashOutcome::SpawnAgentTurnQuiet
        }
        SlashDispatch::Compact { .. } => {
            if ctx.agent_session.is_none() {
                return SlashOutcome::Status("Agent session required for this command.".into());
            }
            let session = ctx.agent_session.clone().expect("checked above");
            let paths = ctx.paths.cloned();
            let cwd = ctx.cwd.map(|path| path.to_path_buf());
            let extension_host = ctx.extension_host.cloned();
            SlashDispatcher::spawn(session, dispatch, extension_host, paths, cwd);
            // `/compact` must not echo a "/compact" user prompt card — the compaction
            // notice already communicates it.
            SlashOutcome::SpawnAgentTurnQuiet
        }
        SlashDispatch::PromptTemplate { ref name, .. } => {
            if ctx.agent_session.is_none() {
                return SlashOutcome::Status("Agent session required for this command.".into());
            }
            let session = ctx.agent_session.clone().expect("checked above");
            let paths = ctx.paths.cloned();
            let cwd = ctx.cwd.map(|path| path.to_path_buf());
            let extension_host = ctx.extension_host.cloned();
            let name = name.clone();
            SlashDispatcher::spawn(session, dispatch, extension_host, paths, cwd);
            SlashOutcome::SpawnAgentTurnPromptTemplate { name }
        }
        SlashDispatch::Goal { .. } | SlashDispatch::Reload | SlashDispatch::Extension { .. } => {
            if ctx.agent_session.is_none() {
                return SlashOutcome::Status("Agent session required for this command.".into());
            }
            let session = ctx.agent_session.clone().expect("checked above");
            let paths = ctx.paths.cloned();
            let cwd = ctx.cwd.map(|path| path.to_path_buf());
            let extension_host = ctx.extension_host.cloned();
            SlashDispatcher::spawn(session, dispatch, extension_host, paths, cwd);
            // Quiet background work: no slash input echo, no prompt-history entry.
            // The task reports via AgentUiEvent (Status / notices), and the agent
            // loop derives busy state — a failure never strands a stale busy UI.
            SlashOutcome::BackgroundTaskQuiet
        }
        SlashDispatch::Skill { ref name, ref args } => {
            if let Some(skills) = ctx.skills
                && let Some(skill) = skills.iter().find(|skill| skill.name == *name)
                && let Some(notice) = elph_agent::skills::skill_args_validation_notice(skill, args)
            {
                return SlashOutcome::Status(notice);
            }
            if ctx.agent_session.is_none() {
                return SlashOutcome::Status("Agent session required for this command.".into());
            }
            let session = ctx.agent_session.clone().expect("checked above");
            let paths = ctx.paths.cloned();
            let cwd = ctx.cwd.map(|path| path.to_path_buf());
            let extension_host = ctx.extension_host.cloned();
            let name = name.clone();
            SlashDispatcher::spawn(session, dispatch, extension_host, paths, cwd);
            SlashOutcome::SpawnAgentTurnSkill { name }
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
            | SlashOutcome::BackgroundTaskQuiet
            | SlashOutcome::OpenModelSelector { .. }
            | SlashOutcome::OpenScopedModels
            | SlashOutcome::OpenSystemPromptDialog { .. }
            | SlashOutcome::OpenViewPlanDialog { .. }
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
            | SlashOutcome::ResumeSession { .. }
            | SlashOutcome::OpenItemSelector { .. }
    )
}

fn open_resume_item_selector(session: Option<&Arc<crate::agent::CodingAgentSession>>) -> SlashOutcome {
    let Some(session) = session else {
        return SlashOutcome::Status("Agent session required for /resume.".into());
    };
    let current = session.session_id().to_string();
    match elph_agent::runtime::try_block_on(async {
        let sm = session.session_manager();
        crate::agent::list_session_select_items(sm)
            .await
            .map_err(|e| format!("list sessions: {e:#}"))
    }) {
        Ok(Ok(items)) if items.is_empty() => SlashOutcome::Status("No sessions for this project yet.".into()),
        Ok(Ok(items)) => SlashOutcome::OpenItemSelector {
            purpose: crate::tui::item_selector::ItemSelectorPurpose::ResumeSession,
            title: "Resume session".into(),
            items,
            preferred_value: Some(current),
            footer_hint: crate::tui::item_selector::default_resume_footer_hint(),
        },
        Ok(Err(message)) => SlashOutcome::Status(message),
        Err(e) => SlashOutcome::Status(format!("/resume failed: {e}")),
    }
}

/// Open the worker chat overlay from `/intercom` (loads peers + history, then the
/// shell renders `WorkerChatOverlay` with the state stored under `pending_worker_chat`).
fn open_worker_chat_slash(session: Option<&Arc<crate::agent::CodingAgentSession>>) -> SlashOutcome {
    let Some(session) = session else {
        return SlashOutcome::Status("Agent session required for /intercom.".into());
    };
    let session = Arc::clone(session);
    match elph_agent::runtime::try_block_on(session.tui_worker_peers()) {
        Ok(Ok(peers)) => SlashOutcome::OpenWorkerChat { peers },
        Ok(Err(e)) => SlashOutcome::Status(format!("/intercom failed: {e:#}")),
        Err(e) => SlashOutcome::Status(format!("/intercom failed: {e:#}")),
    }
}

fn open_tree_item_selector(session: Option<&Arc<crate::agent::CodingAgentSession>>, branch_only: bool) -> SlashOutcome {
    let Some(session) = session else {
        return SlashOutcome::Status("Agent session required for /tree.".into());
    };
    let session = Arc::clone(session);
    match elph_agent::runtime::try_block_on(async {
        let leaf = session.leaf_id().await.ok().flatten();
        let entries = if branch_only {
            session
                .branch_entries()
                .await
                .map_err(|e| format!("branch entries: {e:#}"))?
        } else {
            session
                .session_tree_entries()
                .await
                .map_err(|e| format!("session entries: {e:#}"))?
        };
        let items = crate::agent::list_tree_select_items_with_leaf(&entries, leaf.as_deref());
        Ok::<_, String>((items, leaf))
    }) {
        Ok(Ok((items, _leaf))) if items.is_empty() => {
            SlashOutcome::Status("Session tree is empty — nothing to navigate.".into())
        }
        Ok(Ok((items, leaf))) => SlashOutcome::OpenItemSelector {
            purpose: crate::tui::item_selector::ItemSelectorPurpose::NavigateTree,
            title: if branch_only {
                "Session tree (branch)".into()
            } else {
                "Session tree".into()
            },
            items,
            preferred_value: leaf,
            footer_hint: crate::tui::item_selector::default_tree_footer_hint(),
        },
        Ok(Err(message)) => SlashOutcome::Status(message),
        Err(e) => SlashOutcome::Status(format!("/tree failed: {e}")),
    }
}

/// Whether the slash outcome should show an agent turn indicator.
pub fn slash_echoes_prompt_in_transcript(outcome: &SlashOutcome) -> bool {
    matches!(
        outcome,
        SlashOutcome::SpawnAgentTurn
            | SlashOutcome::SpawnAgentTurnSkill { .. }
            | SlashOutcome::SpawnAgentTurnPromptTemplate { .. }
            | SlashOutcome::BackgroundTask
    )
}

pub fn overlay_deferred_message(overlay: &OverlayCommand) -> String {
    match overlay {
        OverlayCommand::Model { .. } => "/model overlay not yet implemented".into(),
        OverlayCommand::ScopedModels => "/scoped-models overlay not yet implemented".into(),
        OverlayCommand::Tree => "Use /tree for the interactive session tree picker.".into(),
        OverlayCommand::Resume => "Use /resume for the interactive session picker.".into(),
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
        assert!(slash_echoes_prompt_in_transcript(&SlashOutcome::SpawnAgentTurnSkill {
            name: "test".into()
        }));
        assert!(slash_echoes_prompt_in_transcript(&SlashOutcome::SpawnAgentTurnPromptTemplate {
            name: "test".into()
        }));
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
        assert!(!slash_outcome_is_ui_only(&SlashOutcome::SpawnAgentTurnSkill {
            name: "test".into()
        }));
        assert!(!slash_outcome_is_ui_only(&SlashOutcome::SpawnAgentTurnPromptTemplate {
            name: "test".into()
        }));
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
        });
        assert!(matches!(
            outcome,
            SlashOutcome::Unimplemented(message) if message == "not-a-real-command not yet implemented"
        ));
    }

    #[test]
    fn skill_slash_without_session_returns_status() {
        let skill = elph_agent::harness::Skill {
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
        });
        assert!(matches!(
            outcome,
            SlashOutcome::Status(message) if message == "Agent session required for this command."
        ));
    }

    #[test]
    fn skill_slash_missing_required_args_returns_notice() {
        let skill = elph_agent::harness::Skill {
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
        });
        assert!(matches!(
            outcome,
            SlashOutcome::Status(ref message) if message.contains("Unknown builtin provider")
        ));
    }

    #[test]
    fn transfer_codex_without_session_returns_status() {
        let outcome = handle_slash_submit(SlashContext {
            input: "/transfer codex",
            extensions: None,
            prompt_templates: None,
            skills: None,
            agent_session: None,
            extension_host: None,
            paths: None,
            cwd: None,
        });
        assert!(matches!(
            outcome,
            SlashOutcome::Status(ref message) if message == "Agent session required for this command."
        ));
        assert!(slash_outcome_is_ui_only(&outcome));
    }

    #[test]
    fn transfer_missing_tool_shows_usage() {
        let outcome = handle_slash_submit(SlashContext {
            input: "/transfer",
            extensions: None,
            prompt_templates: None,
            skills: None,
            agent_session: None,
            extension_host: None,
            paths: None,
            cwd: None,
        });
        assert!(matches!(
            outcome,
            SlashOutcome::Status(ref message) if message.contains("Usage: /transfer")
        ));
    }

    #[test]
    fn transfer_unknown_tool_shows_usage() {
        let outcome = handle_slash_submit(SlashContext {
            input: "/transfer cursor",
            extensions: None,
            prompt_templates: None,
            skills: None,
            agent_session: None,
            extension_host: None,
            paths: None,
            cwd: None,
        });
        assert!(matches!(
            outcome,
            SlashOutcome::Status(ref message) if message.contains("Unknown transfer tool `cursor`")
        ));
    }

    #[test]
    fn transfer_outcome_is_ui_only_for_codex() {
        assert!(slash_outcome_is_ui_only(&SlashOutcome::Status(
            "Agent session required for this command.".to_string()
        )));
    }

    #[test]
    fn background_task_quiet_does_not_echo_and_is_ui_only() {
        // The transfer dispatch is a quiet background task: the slash input must
        // NOT be echoed as a user card (visible feedback comes from the handoff
        // meta line / stream events), and it must never be treated as an
        // agent-turn spawn (busy is derived from the agent loop itself).
        let outcome = SlashOutcome::BackgroundTaskQuiet;
        assert!(slash_outcome_is_ui_only(&outcome));
        assert!(!slash_echoes_prompt_in_transcript(&outcome));
        // Contrast with the regular background task, which does echo.
        assert!(slash_echoes_prompt_in_transcript(&SlashOutcome::BackgroundTask));
        // Skills and prompt templates do echo (with custom formatting).
        assert!(slash_echoes_prompt_in_transcript(&SlashOutcome::SpawnAgentTurnSkill {
            name: "test".into()
        }));
        assert!(slash_echoes_prompt_in_transcript(&SlashOutcome::SpawnAgentTurnPromptTemplate {
            name: "test".into()
        }));
    }
}
