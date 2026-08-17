//! `session/prompt` accept-immediately + `session/cancel`.

use std::sync::Arc;
use std::sync::atomic::Ordering;

use agent_client_protocol::schema::v2::{CancelSessionNotification, PromptRequest, PromptResponse, StopReason};
use agent_client_protocol::{Client, ConnectionTo};
use parking_lot::Mutex;

use crate::platform::acp::commands;
use crate::platform::acp::content::extract_prompt;
use crate::platform::acp::state::{AcpAgentState, lookup_session, session_key};
use crate::platform::acp::updates::{drive_turn, is_running, send_idle, send_running, send_user_message};

pub async fn handle_prompt(
    state: Arc<Mutex<AcpAgentState>>,
    connection: ConnectionTo<Client>,
    request: PromptRequest,
    responder: agent_client_protocol::Responder<PromptResponse>,
) -> anyhow::Result<()> {
    let session_id = request.session_id.clone();
    let key = session_key(&session_id);
    if let Err(error) = lookup_session(&state, &key) {
        let _ = responder.respond_with_error(agent_client_protocol::util::internal_error(error.to_string()));
        return Ok(());
    }
    let extracted = match extract_prompt(&request.prompt) {
        Ok(extracted) => extracted,
        Err(error) => {
            let _ = responder.respond_with_error(agent_client_protocol::util::internal_error(error));
            return Ok(());
        }
    };
    if !extracted.images.is_empty() {
        let (session, _, _) = lookup_session(&state, &key)?;
        if !session.supports_image_input() {
            let _ = responder.respond_with_error(agent_client_protocol::util::internal_error(
                "this model does not accept image prompt content",
            ));
            return Ok(());
        }
    }
    let _ = responder.respond(PromptResponse::new());
    let user_id = {
        let guard = state.lock();
        guard
            .sessions
            .get(&session_key(&session_id))
            .map(|s| s.ids.next("msg_user"))
            .unwrap_or_else(|| "msg_user_1".into())
    };

    send_user_message(&connection, &session_id, &user_id, request.prompt.clone())?;

    let trimmed = extracted.text.trim();
    if commands::is_slash(trimmed) {
        send_running(&connection, &session_id)?;
        commands::handle_slash(&state, &connection, &session_id, trimmed).await?;
        return Ok(());
    }

    let (session, ui_rx, _) = lookup_session(&state, &key)?;
    let steer = is_running(&state, &session_id);
    let extra = extra_roots_prefix(&state, &key);
    let mut text = extracted.text;
    if !extra.is_empty() {
        text = format!("{extra}\n\n{text}");
    }
    text = hydrate_local_file_links(text);
    let images = extracted
        .images
        .into_iter()
        .map(|(data, mime)| elph_ai::ImageContent::new(data, mime))
        .collect::<Vec<_>>();
    drive_turn(
        &state,
        &connection,
        &session_id,
        session,
        text,
        steer,
        (!images.is_empty()).then_some(images),
        &ui_rx,
    )
    .await
}

fn extra_roots_prefix(state: &Arc<Mutex<AcpAgentState>>, key: &str) -> String {
    let dirs = state
        .lock()
        .sessions
        .get(key)
        .map(|s| s.additional_directories.clone())
        .unwrap_or_default();
    if dirs.is_empty() {
        return String::new();
    }
    let mut lines = vec!["Additional workspace directories:".to_string()];
    for dir in dirs {
        lines.push(format!("- {}", dir.display()));
    }
    lines.join("\n")
}

fn hydrate_local_file_links(text: String) -> String {
    let mut out = text.clone();
    for token in text.split_whitespace() {
        let Some(raw) = token.strip_prefix("(file://").or_else(|| token.strip_prefix("file://")) else {
            continue;
        };
        let path = raw.trim_end_matches(')');
        if let Ok(body) = std::fs::read_to_string(path) {
            let excerpt: String = body.chars().take(8_000).collect();
            out.push_str(&format!("\n\n<resource uri=\"file://{path}\">\n{excerpt}\n</resource>"));
        }
    }
    out
}

pub async fn handle_cancel(
    state: &Arc<Mutex<AcpAgentState>>,
    connection: &ConnectionTo<Client>,
    notification: CancelSessionNotification,
) -> anyhow::Result<()> {
    let key = session_key(&notification.session_id);
    let (session, _, _) = match lookup_session(state, &key) {
        Ok(ctx) => ctx,
        Err(_) => return Ok(()),
    };
    if let Some(entry) = state.lock().sessions.get(&key) {
        entry.cancelled.store(true, Ordering::Relaxed);
    }
    let _ = session.abort().await;
    crate::platform::acp::tools::cancel_open_tools(state, connection, &notification.session_id)?;
    send_idle(connection, &notification.session_id, StopReason::Cancelled)?;
    Ok(())
}
