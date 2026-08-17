//! Slash command advertisement and ACP-safe dispatch.

use agent_client_protocol::schema::v2::{
    AvailableCommand, AvailableCommandInput, AvailableCommandsUpdate, SessionId, SessionUpdate, TextCommandInput,
};
use agent_client_protocol::{Client, ConnectionTo};

use crate::agent::{
    CodingAgentSession, SlashDispatch, clone_session_message, dispatch_slash_command, export_session_message,
    fork_session_message, format_help_message, import_session_from_jsonl, import_slash_message, resume_list_message,
    slash_commands_for_palette, system_prompt_slash_message, tree_slash_message, trust_slash_message,
    workers_slash_message,
};
use crate::platform::Paths;
use crate::platform::acp::state::{AcpAgentState, lookup_session};
use crate::platform::acp::updates::{send_agent_text, send_idle, send_update};
use crate::types::{SlashCommand, SlashCommandKind};

use parking_lot::Mutex;
use std::sync::Arc;

/// Headless-safe builtins only. TUI pickers/overlays are omitted (model, resume, memory, …).
const ACP_COMMANDS: &[(&str, &str, Option<&str>)] = &[
    ("help", "List commands", None),
    ("tools", "List available tools", Some("[filter]")),
    ("system-prompt", "Show compiled system prompt", None),
    ("session", "Show session info", None),
    ("rename", "Rename the current session", Some("<title>")),
    ("compact", "Compact conversation history", Some("[args]")),
    ("continue", "Resume the interrupted task", None),
    ("reload", "Reload workspace resources", None),
    ("goal", "Inspect or update the session goal", Some("[args]")),
    ("changelog", "Show changelog", None),
    ("export", "Export session as JSONL", Some("[path]")),
    ("import", "Import session JSONL", Some("[path]")),
    ("trust", "Save project trust decision", None),
    ("aside", "Ask a side question without interrupting", Some("<question>")),
    ("mcp", "List or logout MCP servers", Some("[list|logout <name>]")),
];

/// ACP-safe builtins plus session prompt templates and skills (pi-acp style).
pub async fn advertised_commands(session: &CodingAgentSession) -> Vec<AvailableCommand> {
    to_v2_commands(&slash_catalog(session).await)
}

pub async fn send_available_commands(
    connection: &ConnectionTo<Client>,
    session_id: &SessionId,
    session: &CodingAgentSession,
) -> anyhow::Result<()> {
    send_update(
        connection,
        session_id,
        SessionUpdate::AvailableCommandsUpdate(AvailableCommandsUpdate::new(advertised_commands(session).await)),
    )
}

pub struct AdvertisedSlash {
    pub name: String,
    pub description: String,
    pub hint: Option<String>,
}

pub async fn slash_catalog(session: &CodingAgentSession) -> Vec<AdvertisedSlash> {
    let resources = session.harness().get_resources().await;
    let palette = slash_commands_for_palette(
        None,
        Some(resources.prompt_templates.as_slice()),
        Some(resources.skills.as_slice()),
    );
    merge_advertised(&palette)
}

fn merge_advertised(palette: &[SlashCommand]) -> Vec<AdvertisedSlash> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for (name, desc, hint) in ACP_COMMANDS {
        seen.insert((*name).to_string());
        out.push(AdvertisedSlash {
            name: (*name).to_string(),
            description: (*desc).to_string(),
            hint: hint.map(str::to_string),
        });
    }
    for cmd in palette.iter().filter(|c| !c.hidden) {
        let (name, description) = match cmd.kind {
            SlashCommandKind::PromptTemplate => (cmd.name.clone(), cmd.description.clone()),
            SlashCommandKind::Skill => (format!("skill:{}", cmd.name), cmd.description.clone()),
            SlashCommandKind::Builtin | SlashCommandKind::Extension => continue,
        };
        if !seen.insert(name.clone()) {
            continue;
        }
        out.push(AdvertisedSlash {
            name,
            description,
            hint: cmd.args_hint.clone(),
        });
    }
    out
}

fn to_v2_commands(cmds: &[AdvertisedSlash]) -> Vec<AvailableCommand> {
    cmds.iter()
        .map(|c| {
            let mut cmd = AvailableCommand::new(c.name.clone(), c.description.clone());
            if let Some(hint) = &c.hint {
                cmd = cmd.input(AvailableCommandInput::Text(TextCommandInput::new(hint.clone())));
            }
            cmd
        })
        .collect()
}

/// Outcome of an ACP-safe slash command (no wire types).
pub enum SlashOutcome {
    /// Reply with this text and end the turn.
    Text(String),
    /// Submit `input` as a model prompt.
    SubmitPrompt,
    /// Expand and run a workspace skill (`harness.skill`).
    Skill { name: String, args: String },
    /// Expand and run a prompt template.
    PromptTemplate { name: String, args: String },
    /// Continue the interrupted task (`/continue`).
    Continue,
    /// Reload finished; re-advertise slash commands to the client.
    Reloaded(String),
}

pub async fn resolve_slash(state: &Arc<Mutex<AcpAgentState>>, key: &str, input: &str) -> anyhow::Result<SlashOutcome> {
    let (session, _, _) = lookup_session(state, key)?;
    let resources = session.harness().get_resources().await;
    let templates = resources.prompt_templates.clone();
    let skills = resources.skills.clone();
    let dispatch = dispatch_slash_command(input, None, Some(&templates), Some(&skills));

    let text = match dispatch {
        Some(SlashDispatch::Help) => format_help_message(None, Some(&templates), Some(&skills)),
        Some(SlashDispatch::Tools { .. }) => {
            let (session, _, _) = lookup_session(state, key)?;
            crate::agent::discovery_tools_message(&session)
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))?
        }
        Some(SlashDispatch::SystemPrompt) => {
            let (session, _, _) = lookup_session(state, key)?;
            system_prompt_slash_message(Some(&session)).map_err(|e| anyhow::anyhow!("{e}"))?
        }
        Some(SlashDispatch::SessionInfo) => {
            let (session, _, _) = lookup_session(state, key)?;
            crate::agent::session_info_slash_message(Some(&session), None).map_err(|e| anyhow::anyhow!("{e}"))?
        }
        Some(SlashDispatch::Rename { args }) => {
            let title = args.trim();
            if title.is_empty() {
                "Usage: /rename <title>".into()
            } else {
                let (session, _, _) = lookup_session(state, key)?;
                crate::agent::rename_session_title(&session, title).map_err(|e| anyhow::anyhow!("{e}"))?;
                format!("Session renamed to “{title}”.")
            }
        }
        Some(SlashDispatch::Compact { .. }) => {
            let (session, _, _) = lookup_session(state, key)?;
            session.compact().await?;
            "History compacted.".into()
        }
        Some(SlashDispatch::Reload) => {
            let (session, _, cwd) = lookup_session(state, key)?;
            let paths = state.lock().paths.clone();
            let report = session
                .reload_workspace(crate::agent::WorkspaceReloadRequest {
                    paths: &paths,
                    cwd: &cwd,
                })
                .await;
            let _ = session.reconcile_tool_surface().await;
            let mut parts = vec![report.summary_text()];
            parts.extend(report.notices);
            return Ok(SlashOutcome::Reloaded(parts.join("\n\n")));
        }
        Some(SlashDispatch::Goal { args }) => {
            let (session, _, _) = lookup_session(state, key)?;
            crate::agent::goal_slash::handle_goal_slash(session.goal_runtime().as_ref(), &args)
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))?
        }
        Some(SlashDispatch::Continue) => return Ok(SlashOutcome::Continue),
        Some(SlashDispatch::Quit) => "Goodbye! Close the ACP session from the client.".into(),
        Some(SlashDispatch::NewSession) => "Create a new conversation with session/new instead of /new.".into(),
        Some(SlashDispatch::Resume { args }) => {
            if args.trim().is_empty() {
                let (session, _, _) = lookup_session(state, key)?;
                resume_list_message(&session)
                    .await
                    .map_err(|e| anyhow::anyhow!("{e}"))?
            } else {
                format!(
                    "Switching sessions uses session/resume. Reconnect with session id `{}`.",
                    args.trim()
                )
            }
        }
        Some(SlashDispatch::ProviderList) => crate::tui::slash_handler::provider_list_slash_message(),
        Some(SlashDispatch::McpList) => {
            let paths = state.lock().paths.clone();
            crate::tui::mcp_auth_dialog::mcp_list_slash_message(&paths)
        }
        Some(SlashDispatch::McpLogout { server_name }) => match server_name {
            None => "Usage: /mcp logout <server>".into(),
            Some(name) => {
                let paths = state.lock().paths.clone();
                crate::tui::mcp_auth_dialog::logout_mcp_server(&paths, &name).map_err(|e| anyhow::anyhow!("{e}"))?
            }
        },
        Some(SlashDispatch::Hotkeys) => crate::agent::HOTKEYS_TEXT.to_string(),
        Some(SlashDispatch::Changelog) => crate::agent::changelog_text(),
        Some(SlashDispatch::Settings) => {
            let paths = state.lock().paths.clone();
            crate::agent::settings_slash_message(&paths)
        }
        Some(SlashDispatch::Workers) => {
            let (session, _, _) = lookup_session(state, key)?;
            workers_slash_message(Some(&session))
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))?
        }
        Some(SlashDispatch::Tree { args }) => {
            let (session, _, _) = lookup_session(state, key)?;
            tree_slash_message(&session, &args)
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))?
        }
        Some(SlashDispatch::Export { args }) => {
            let (session, _, cwd) = lookup_session(state, key)?;
            export_session_message(&session, &cwd, &args)
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))?
        }
        Some(SlashDispatch::Import { args }) => {
            if args.trim().is_empty() {
                import_slash_message(&args)
            } else {
                let (session, _, cwd) = lookup_session(state, key)?;
                match import_session_from_jsonl(&session, &cwd, &args).await {
                    Ok((message, new_id)) => {
                        format!("{message}\n\nACP note: reconnect with session/resume `{new_id}`.")
                    }
                    Err(e) => e,
                }
            }
        }
        Some(SlashDispatch::Trust) => {
            let (_, _, cwd) = lookup_session(state, key)?;
            let paths = Paths::resolve().map_err(|e| anyhow::anyhow!("{e}"))?;
            trust_slash_message(&paths, &cwd).map_err(|e| anyhow::anyhow!("{e}"))?
        }
        Some(SlashDispatch::Fork) => {
            let (session, _, _) = lookup_session(state, key)?;
            fork_session_message(&session)
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))?
        }
        Some(SlashDispatch::CloneSession) => {
            let (session, _, _) = lookup_session(state, key)?;
            clone_session_message(&session)
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))?
        }
        Some(SlashDispatch::Aside { question }) => {
            let question = question.trim().to_string();
            if question.is_empty() {
                "Usage: /aside <question>".into()
            } else {
                let (session, _, _) = lookup_session(state, key)?;
                return Ok(SlashOutcome::Text(
                    match crate::agent::aside_answer(&session, &question).await {
                        Ok(answer) => format!("/aside {question}\n\n{answer}"),
                        Err(error) => format!("/aside error: {error}"),
                    },
                ));
            }
        }
        Some(SlashDispatch::Skill { name, args }) => return Ok(SlashOutcome::Skill { name, args }),
        Some(SlashDispatch::PromptTemplate { name, args }) => {
            return Ok(SlashOutcome::PromptTemplate { name, args });
        }
        Some(SlashDispatch::Extension { .. }) | None => return Ok(SlashOutcome::SubmitPrompt),
        Some(other) => tui_only_message(&other),
    };

    Ok(SlashOutcome::Text(text))
}

pub async fn handle_slash(
    state: &Arc<Mutex<AcpAgentState>>,
    connection: &ConnectionTo<Client>,
    session_id: &SessionId,
    input: &str,
) -> anyhow::Result<()> {
    let key = session_id.0.as_ref();
    match resolve_slash(state, key, input).await? {
        SlashOutcome::Text(text) => {
            send_agent_text(connection, session_id, &text)?;
            send_idle(connection, session_id, agent_client_protocol::schema::v2::StopReason::EndTurn)?;
            Ok(())
        }
        SlashOutcome::Continue => {
            let (session, ui_rx, _) = lookup_session(state, key)?;
            crate::platform::acp::updates::drive_turn(
                state,
                connection,
                session_id,
                session,
                crate::agent::RETRY_CONTINUE_PROMPT.to_string(),
                false,
                None,
                &ui_rx,
            )
            .await
        }
        SlashOutcome::SubmitPrompt => {
            let (session, ui_rx, _) = lookup_session(state, key)?;
            crate::platform::acp::updates::drive_turn(
                state,
                connection,
                session_id,
                session,
                input.to_string(),
                false,
                None,
                &ui_rx,
            )
            .await
        }
        SlashOutcome::Skill { name, args } => {
            let (session, ui_rx, _) = lookup_session(state, key)?;
            crate::platform::acp::updates::drive_skill(state, connection, session_id, session, name, args, &ui_rx).await
        }
        SlashOutcome::PromptTemplate { name, args } => {
            let (session, ui_rx, _) = lookup_session(state, key)?;
            crate::platform::acp::updates::drive_template(state, connection, session_id, session, name, args, &ui_rx)
                .await
        }
        SlashOutcome::Reloaded(text) => {
            if let Ok((session, _, _)) = lookup_session(state, key) {
                let _ = send_available_commands(connection, session_id, &session).await;
            }
            send_agent_text(connection, session_id, &text)?;
            send_idle(connection, session_id, agent_client_protocol::schema::v2::StopReason::EndTurn)?;
            Ok(())
        }
    }
}

fn tui_only_message(dispatch: &SlashDispatch) -> String {
    match dispatch {
        SlashDispatch::Confetti { .. } => "Confetti is TUI-only.".into(),
        SlashDispatch::Memory { .. } => "/memory is TUI-only. Use `elph memory`.".into(),
        SlashDispatch::OverlayNeeded(overlay) => format!("Command '{overlay:?}' requires the TUI."),
        SlashDispatch::Feedback => "/feedback is TUI-only.".into(),
        SlashDispatch::ProviderConnect { .. } => "Use `elph provider connect` — ACP cannot open the TUI dialog.".into(),
        SlashDispatch::ProviderDisconnect { .. } => {
            "Use `elph provider disconnect` — ACP cannot open the TUI dialog.".into()
        }
        SlashDispatch::ProviderUpdate { .. } => "Use `elph provider update`.".into(),
        SlashDispatch::McpAuth { .. } => "Use `elph mcp auth <name>` — ACP cannot open the TUI OAuth dialog.".into(),
        SlashDispatch::WorkerChat => "/intercom is TUI-only.".into(),
        SlashDispatch::Handover { .. } => "/handover is TUI-only.".into(),
        SlashDispatch::Unimplemented(cmd) => format!("Slash command '{cmd}' is not available via ACP."),
        other => format!("Command {other:?} is not available via ACP."),
    }
}

pub fn is_slash(input: &str) -> bool {
    let trimmed = input.trim();
    trimmed.starts_with('/') && trimmed.len() > 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_slash_commands() {
        assert!(is_slash("/help"));
        assert!(is_slash("  /tools  "));
        assert!(!is_slash("/"));
        assert!(!is_slash("hello"));
    }

    #[test]
    fn catalog_includes_templates_and_prefixed_skills() {
        let palette = vec![
            SlashCommand::new("review", "[prompt] Review a PR")
                .with_kind(SlashCommandKind::PromptTemplate)
                .with_args_hint("<url>"),
            SlashCommand::new("code-review", "[skill] Review changes").with_kind(SlashCommandKind::Skill),
            SlashCommand::new("model", "Select model"),
            SlashCommand::new("help", "duplicate builtin"),
        ];
        let cmds = merge_advertised(&palette);
        assert!(cmds.iter().any(|c| c.name == "help"));
        assert!(
            cmds.iter()
                .any(|c| c.name == "review" && c.hint.as_deref() == Some("<url>"))
        );
        assert!(cmds.iter().any(|c| c.name == "skill:code-review"));
        assert!(!cmds.iter().any(|c| c.name == "code-review"));
        assert!(!cmds.iter().any(|c| c.name == "model"));
        assert!(!cmds.iter().any(|c| c.name == "hotkeys" || c.name == "workers"));
        assert_eq!(cmds.iter().filter(|c| c.name == "help").count(), 1);
    }
}
