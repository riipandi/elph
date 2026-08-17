//! Slash command advertisement and ACP-safe dispatch.

use agent_client_protocol::schema::v2::{
    AvailableCommand, AvailableCommandInput, AvailableCommandsUpdate, SessionId, SessionUpdate, TextCommandInput,
};
use agent_client_protocol::{Client, ConnectionTo};

use crate::agent::{
    SlashDispatch, clone_session_message, dispatch_slash_command, export_session_message, fork_session_message,
    format_help_message, import_session_from_jsonl, import_slash_message, resume_list_message,
    system_prompt_slash_message, tools_slash_message, tree_slash_message, trust_slash_message, workers_slash_message,
};
use crate::platform::Paths;
use crate::platform::acp::state::{AcpAgentState, lookup_session};
use crate::platform::acp::updates::{send_agent_text, send_idle, send_update};

use parking_lot::Mutex;
use std::sync::Arc;

const ACP_COMMANDS: &[(&str, &str, Option<&str>)] = &[
    ("help", "List commands", None),
    ("tools", "List available tools", Some("[filter]")),
    ("session", "Show session info", None),
    ("rename", "Rename the current session", Some("<title>")),
    ("compact", "Compact conversation history", Some("[args]")),
    ("continue", "Resume the interrupted task", None),
    ("reload", "Reload workspace resources", None),
    ("goal", "Inspect or update the session goal", Some("[args]")),
    ("settings", "Show settings paths", None),
    ("changelog", "Show changelog", None),
    ("hotkeys", "Show keyboard shortcuts", None),
    ("workers", "List live multi-worker peers", None),
    ("tree", "Navigate session tree", Some("[entry_id]")),
    ("export", "Export session as JSONL", Some("[path]")),
    ("import", "Import session JSONL", Some("[path]")),
    ("trust", "Save project trust decision", None),
    ("fork", "Fork the current session", None),
    ("clone", "Clone the current session", None),
    ("aside", "Ask a side question without interrupting", Some("<question>")),
    ("mcp", "List or logout MCP servers", Some("[list|logout <name>]")),
    ("provider", "List configured providers", Some("list")),
];

pub fn advertised_commands() -> Vec<AvailableCommand> {
    ACP_COMMANDS
        .iter()
        .map(|(name, desc, hint)| {
            let mut cmd = AvailableCommand::new(*name, *desc);
            if let Some(hint) = hint {
                cmd = cmd.input(AvailableCommandInput::Text(TextCommandInput::new(*hint)));
            }
            cmd
        })
        .collect()
}

pub fn send_available_commands(connection: &ConnectionTo<Client>, session_id: &SessionId) -> anyhow::Result<()> {
    send_update(
        connection,
        session_id,
        SessionUpdate::AvailableCommandsUpdate(AvailableCommandsUpdate::new(advertised_commands())),
    )
}

pub async fn handle_slash(
    state: &Arc<Mutex<AcpAgentState>>,
    connection: &ConnectionTo<Client>,
    session_id: &SessionId,
    input: &str,
) -> anyhow::Result<()> {
    let key = session_id.0.as_ref();
    let dispatch = dispatch_slash_command(input, None, None, None);

    let text = match dispatch {
        Some(SlashDispatch::Help) => format_help_message(None, None, None),
        Some(SlashDispatch::Tools { .. }) => {
            let (session, _, _) = lookup_session(state, key)?;
            tools_slash_message(Some(&session)).map_err(|e| anyhow::anyhow!("{e}"))?
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
            let mut parts = vec![report.summary_text()];
            parts.extend(report.notices);
            let _ = send_available_commands(connection, session_id);
            parts.join("\n\n")
        }
        Some(SlashDispatch::Goal { args }) => {
            let (session, _, _) = lookup_session(state, key)?;
            crate::agent::goal_slash::handle_goal_slash(session.goal_runtime().as_ref(), &args)
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))?
        }
        Some(SlashDispatch::Continue) => {
            let (session, ui_rx, _) = lookup_session(state, key)?;
            session
                .submit_prompt(crate::agent::RETRY_CONTINUE_PROMPT.to_string(), false)
                .await?;
            crate::platform::acp::updates::stream_ui_events(state, connection, session_id, &ui_rx).await?;
            return Ok(());
        }
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
                let (session, ui_rx, _) = lookup_session(state, key)?;
                let _ = crate::agent::spawn_aside(session, question);
                let mut rx = ui_rx.lock().await;
                loop {
                    let Some(event) = rx.recv().await else {
                        break;
                    };
                    match event {
                        crate::agent::AgentUiEvent::AsideFinished { answer, question, .. } => {
                            send_agent_text(connection, session_id, &format!("/aside {question}\n\n{answer}"))?;
                            send_idle(connection, session_id, agent_client_protocol::schema::v2::StopReason::EndTurn)?;
                            return Ok(());
                        }
                        crate::agent::AgentUiEvent::AsideFailed { error, .. } => {
                            send_agent_text(connection, session_id, &format!("/aside error: {error}"))?;
                            send_idle(connection, session_id, agent_client_protocol::schema::v2::StopReason::EndTurn)?;
                            return Ok(());
                        }
                        _ => {}
                    }
                }
                return Ok(());
            }
        }
        Some(SlashDispatch::Skill { .. }) => {
            let (session, ui_rx, _) = lookup_session(state, key)?;
            session.submit_prompt(input.to_string(), false).await?;
            crate::platform::acp::updates::stream_ui_events(state, connection, session_id, &ui_rx).await?;
            return Ok(());
        }
        Some(SlashDispatch::PromptTemplate { name, .. }) => {
            let _ = name;
            let (session, ui_rx, _) = lookup_session(state, key)?;
            session.submit_prompt(input.to_string(), false).await?;
            crate::platform::acp::updates::stream_ui_events(state, connection, session_id, &ui_rx).await?;
            return Ok(());
        }
        Some(SlashDispatch::Extension { name, .. }) => {
            let _ = name;
            let (session, ui_rx, _) = lookup_session(state, key)?;
            session.submit_prompt(input.to_string(), false).await?;
            crate::platform::acp::updates::stream_ui_events(state, connection, session_id, &ui_rx).await?;
            return Ok(());
        }
        Some(other) => tui_only_message(&other),
        None => {
            let (session, ui_rx, _) = lookup_session(state, key)?;
            session.submit_prompt(input.to_string(), false).await?;
            crate::platform::acp::updates::stream_ui_events(state, connection, session_id, &ui_rx).await?;
            return Ok(());
        }
    };

    send_agent_text(connection, session_id, &text)?;
    send_idle(connection, session_id, agent_client_protocol::schema::v2::StopReason::EndTurn)?;
    Ok(())
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
