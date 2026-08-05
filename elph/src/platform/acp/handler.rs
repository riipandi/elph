//! ACP request handlers — prompt dispatch and slash command routing.

use std::sync::Arc;

use agent_client_protocol::schema::v1::{PromptRequest, SessionId};
use agent_client_protocol::{Client, ConnectionTo};
use parking_lot::Mutex;

use super::util::{extract_prompt_text, send_text_chunks, stream_ui_events};
use super::{AcpAgentState, lookup_session};
use crate::agent::{
    SlashDispatch, dispatch_slash_command, format_help_message, system_prompt_slash_message, tools_slash_message,
};

/// Handle an incoming `session/prompt` request.
///
/// Intercepts slash commands (`/help`, `/tools`, etc.) and handles them
/// without going through the LLM. Everything else is submitted as a normal
/// prompt and streamed back as notification chunks.
pub(super) async fn run_prompt(
    state: &Arc<Mutex<AcpAgentState>>,
    connection: &ConnectionTo<Client>,
    session_id: &SessionId,
    request: &PromptRequest,
) -> anyhow::Result<()> {
    let prompt = extract_prompt_text(request);
    let trimmed = prompt.trim();

    if trimmed.starts_with('/') && trimmed.len() > 1 {
        return handle_acp_slash_command(state, connection, session_id, trimmed).await;
    }

    let key = session_id.0.to_string();
    let (session, ui_rx, _cwd) = lookup_session(state, &key)?;

    session.submit_prompt(prompt, false).await?;
    stream_ui_events(connection, session_id, &ui_rx).await
}

/// Dispatch a slash command received via ACP.
///
/// Only built-in commands are supported; extension, prompt-template, and skill
/// commands are unavailable because the ACP server does not load those resources
/// client-side. UI-only commands (confetti, overlay) return an explanatory error.
async fn handle_acp_slash_command(
    state: &Arc<Mutex<AcpAgentState>>,
    connection: &ConnectionTo<Client>,
    session_id: &SessionId,
    input: &str,
) -> anyhow::Result<()> {
    let key = session_id.0.to_string();
    let dispatch = dispatch_slash_command(input, None, None, None);

    match dispatch {
        // ── Text-producing commands ──────────────────────────────────────────
        Some(SlashDispatch::Help) => {
            let help = format_help_message(None, None, None);
            send_text_chunks(connection, session_id, &help).await
        }
        Some(SlashDispatch::Tools { .. }) => {
            let (session, _, _) = lookup_session(state, &key)?;
            let message = tools_slash_message(Some(&session)).map_err(|e| anyhow::anyhow!("{e}"))?;
            send_text_chunks(connection, session_id, &message).await
        }
        Some(SlashDispatch::SystemPrompt) => {
            let (session, _, _) = lookup_session(state, &key)?;
            let message = system_prompt_slash_message(Some(&session)).map_err(|e| anyhow::anyhow!("{e}"))?;
            send_text_chunks(connection, session_id, &message).await
        }
        Some(SlashDispatch::SessionInfo) => {
            let (session, _, _) = lookup_session(state, &key)?;
            let message =
                crate::agent::session_info_slash_message(Some(&session), None).map_err(|e| anyhow::anyhow!("{e}"))?;
            send_text_chunks(connection, session_id, &message).await
        }
        Some(SlashDispatch::Rename { args }) => {
            let (session, _, _) = lookup_session(state, &key)?;
            let title = args.trim();
            if title.is_empty() {
                return Err(anyhow::anyhow!(
                    "Usage: /rename <title> (interactive rename dialog is TUI-only)."
                ));
            }
            crate::agent::rename_session_title(&session, title).map_err(|e| anyhow::anyhow!("{e}"))?;
            send_text_chunks(connection, session_id, &format!("Session renamed to “{title}”.")).await
        }

        // ── Agent-action commands (run through session) ──────────────────────
        Some(SlashDispatch::Compact) => {
            let (session, _ui_rx, _) = lookup_session(state, &key)?;
            session.compact().await?;
            Ok(())
        }
        Some(SlashDispatch::Reload) => {
            let (session, _, cwd) = lookup_session(state, &key)?;
            let paths = {
                let guard = state.lock();
                guard.paths.clone()
            };
            let report = session
                .reload_workspace(crate::agent::WorkspaceReloadRequest {
                    paths: &paths,
                    cwd: &cwd,
                })
                .await;
            let mut parts = vec![report.summary_text()];
            parts.extend(report.notices);
            send_text_chunks(connection, session_id, &parts.join("\n\n")).await
        }
        Some(SlashDispatch::Goal { args }) => {
            let (session, _, _) = lookup_session(state, &key)?;
            let message = crate::agent::goal_slash::handle_goal_slash(session.goal_runtime().as_ref(), &args)
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            send_text_chunks(connection, session_id, &message).await
        }
        Some(SlashDispatch::Continue) => {
            let (session, _, _) = lookup_session(state, &key)?;
            session
                .submit_prompt(crate::agent::RETRY_CONTINUE_PROMPT.to_string(), false)
                .await?;
            Ok(())
        }

        // ── Informational end-turn ───────────────────────────────────────────
        Some(SlashDispatch::Quit) => send_text_chunks(connection, session_id, "Goodbye!").await,
        Some(SlashDispatch::NewSession) => {
            send_text_chunks(
                connection,
                session_id,
                "New session requested. Use the ACP NewSession endpoint to create one.",
            )
            .await
        }

        // ── Unavailable via ACP ──────────────────────────────────────────────
        Some(SlashDispatch::Confetti { .. }) => {
            Err(anyhow::anyhow!("Confetti is a UI-only command and not available via ACP."))
        }
        Some(SlashDispatch::Memory { .. }) => Err(anyhow::anyhow!("/memory is not available via ACP.")),
        Some(SlashDispatch::Extension { name, .. }) => {
            Err(anyhow::anyhow!("Extension command '{name}' is not available via ACP."))
        }
        Some(SlashDispatch::PromptTemplate { name, .. }) => {
            Err(anyhow::anyhow!("Prompt template '{name}' is not available via ACP."))
        }
        Some(SlashDispatch::Skill { name, .. }) => Err(anyhow::anyhow!("Skill '{name}' is not available via ACP.")),
        Some(SlashDispatch::OverlayNeeded(overlay)) => Err(anyhow::anyhow!(
            "Command '{overlay:?}' requires a TUI overlay and is not available via ACP."
        )),
        Some(SlashDispatch::Feedback) => Err(anyhow::anyhow!(
            "Command '/feedback' opens a browser dialog and is not available via ACP."
        )),
        Some(SlashDispatch::ProviderConnect { .. }) => Err(anyhow::anyhow!(
            "Command '/provider connect' opens a provider selection dialog and is not available via ACP."
        )),
        Some(SlashDispatch::ProviderDisconnect { .. }) => Err(anyhow::anyhow!(
            "Command '/provider disconnect' opens a provider selection dialog and is not available via ACP."
        )),
        Some(SlashDispatch::ProviderList) => {
            let message = crate::tui::slash_handler::provider_list_slash_message();
            send_text_chunks(connection, session_id, &message).await
        }
        Some(SlashDispatch::ProviderUpdate { .. }) => Err(anyhow::anyhow!(
            "Command '/provider update' writes local catalog files and is not available via ACP. Use `elph provider update`."
        )),
        Some(SlashDispatch::McpAuth { .. }) => Err(anyhow::anyhow!(
            "Command '/mcp auth' opens a TUI OAuth dialog and is not available via ACP. Use `elph mcp auth <name>`."
        )),
        Some(SlashDispatch::McpLogout { server_name }) => {
            let Some(name) = server_name else {
                return Err(anyhow::anyhow!("Usage: /mcp logout <server>"));
            };
            let paths = {
                let guard = state.lock();
                guard.paths.clone()
            };
            let message =
                crate::tui::mcp_auth_dialog::logout_mcp_server(&paths, &name).map_err(|e| anyhow::anyhow!("{e}"))?;
            send_text_chunks(connection, session_id, &message).await
        }
        Some(SlashDispatch::McpList) => {
            let paths = {
                let guard = state.lock();
                guard.paths.clone()
            };
            let message = crate::tui::mcp_auth_dialog::mcp_list_slash_message(&paths);
            send_text_chunks(connection, session_id, &message).await
        }
        Some(SlashDispatch::Unimplemented(cmd)) => {
            Err(anyhow::anyhow!("Slash command '{cmd}' is not available via ACP."))
        }

        // ── Not actually a slash command → submit as prompt ──────────────────
        None => {
            let (session, ui_rx, _) = lookup_session(state, &key)?;
            session.submit_prompt(input.to_string(), false).await?;
            stream_ui_events(connection, session_id, &ui_rx).await
        }
    }
}
