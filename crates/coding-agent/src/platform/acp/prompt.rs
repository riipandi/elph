//! `session/prompt` accept-immediately + `session/cancel`.

use std::sync::Arc;
use std::sync::atomic::Ordering;

use agent_client_protocol::schema::v2::{CancelSessionNotification, PromptRequest, PromptResponse, StopReason};
use agent_client_protocol::{Client, ConnectionTo};
use parking_lot::Mutex;

use crate::platform::acp::commands;
use crate::platform::acp::content::extract_prompt;
use crate::platform::acp::state::{AcpAgentState, lookup_session, session_key};
use crate::platform::acp::updates::{
    is_running, mark_running, send_idle, send_running, send_user_message, stream_ui_events,
};

pub async fn handle_prompt(
    state: Arc<Mutex<AcpAgentState>>,
    connection: ConnectionTo<Client>,
    request: PromptRequest,
    responder: agent_client_protocol::Responder<PromptResponse>,
) -> anyhow::Result<()> {
    let session_id = request.session_id.clone();
    let key = session_key(&session_id);
    lookup_session(&state, &key)?;
    let extracted = extract_prompt(&request.prompt).map_err(|e| anyhow::anyhow!("{e}"))?;
    if !extracted.images.is_empty() {
        let (session, _, _) = lookup_session(&state, &key)?;
        if !session.supports_image_input() {
            anyhow::bail!("this model does not accept image prompt content");
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
    mark_running(&state, &session_id);
    send_running(&connection, &session_id)?;
    session.submit_prompt(extracted.text, steer).await?;
    stream_ui_events(&state, &connection, &session_id, &ui_rx).await
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
