//! Map `AgentUiEvent` onto ACP v2 `session/update` notifications.

use std::sync::Arc;
use std::sync::atomic::Ordering;

use agent_client_protocol::schema::v2::{
    AgentMessage, ContentBlock, ContentChunk, IdleStateUpdate, RequiresActionStateUpdate, RunningStateUpdate,
    SessionId, SessionUpdate, StateUpdate, StopReason, TextContent, UpdateSessionNotification, UsageUpdate,
};
use agent_client_protocol::{Client, ConnectionTo};
use parking_lot::Mutex;
use tokio::sync::mpsc;

use crate::agent::{AgentUiEvent, ToolApprovalChoice};
use crate::platform::acp::permission;
use crate::platform::acp::plan;
use crate::platform::acp::state::{AcpAgentState, session_key};
use crate::platform::acp::tools;

pub fn send_update(
    connection: &ConnectionTo<Client>,
    session_id: &SessionId,
    update: SessionUpdate,
) -> anyhow::Result<()> {
    connection
        .send_notification(UpdateSessionNotification::new(session_id.clone(), update))
        .map_err(|e| anyhow::anyhow!("{e}"))
}

pub fn send_running(connection: &ConnectionTo<Client>, session_id: &SessionId) -> anyhow::Result<()> {
    send_update(
        connection,
        session_id,
        SessionUpdate::StateUpdate(StateUpdate::Running(RunningStateUpdate::new())),
    )
}

pub fn send_requires_action(connection: &ConnectionTo<Client>, session_id: &SessionId) -> anyhow::Result<()> {
    send_update(
        connection,
        session_id,
        SessionUpdate::StateUpdate(StateUpdate::RequiresAction(RequiresActionStateUpdate::new())),
    )
}

pub fn send_idle(connection: &ConnectionTo<Client>, session_id: &SessionId, reason: StopReason) -> anyhow::Result<()> {
    send_update(
        connection,
        session_id,
        SessionUpdate::StateUpdate(StateUpdate::Idle(IdleStateUpdate::new().stop_reason(reason))),
    )
}

pub fn send_user_message(
    connection: &ConnectionTo<Client>,
    session_id: &SessionId,
    message_id: &str,
    content: Vec<ContentBlock>,
) -> anyhow::Result<()> {
    send_update(
        connection,
        session_id,
        SessionUpdate::UserMessage(agent_client_protocol::schema::v2::UserMessage::new(message_id).content(content)),
    )
}

pub fn send_agent_text(connection: &ConnectionTo<Client>, session_id: &SessionId, text: &str) -> anyhow::Result<()> {
    if text.is_empty() {
        return Ok(());
    }
    send_update(
        connection,
        session_id,
        SessionUpdate::AgentMessage(
            AgentMessage::new("msg_slash").content(vec![ContentBlock::Text(TextContent::new(text.to_string()))]),
        ),
    )
}

pub fn send_agent_chunk(
    connection: &ConnectionTo<Client>,
    session_id: &SessionId,
    message_id: &str,
    text: String,
) -> anyhow::Result<()> {
    send_update(
        connection,
        session_id,
        SessionUpdate::AgentMessageChunk(ContentChunk::new(ContentBlock::Text(TextContent::new(text)), message_id)),
    )
}

pub fn send_thought_chunk(
    connection: &ConnectionTo<Client>,
    session_id: &SessionId,
    message_id: &str,
    text: String,
) -> anyhow::Result<()> {
    send_update(
        connection,
        session_id,
        SessionUpdate::AgentThoughtChunk(ContentChunk::new(ContentBlock::Text(TextContent::new(text)), message_id)),
    )
}

/// Run a harness turn while streaming UI events (required for tool approval).
#[allow(clippy::too_many_arguments)]
pub async fn drive_turn(
    state: &Arc<Mutex<AcpAgentState>>,
    connection: &ConnectionTo<Client>,
    session_id: &SessionId,
    session: Arc<crate::agent::CodingAgentSession>,
    text: String,
    steer: bool,
    images: Option<Vec<elph_ai::ImageContent>>,
    ui_rx: &Arc<tokio::sync::Mutex<mpsc::UnboundedReceiver<AgentUiEvent>>>,
) -> anyhow::Result<()> {
    mark_running(state, session_id);
    send_running(connection, session_id)?;
    let submit = session.submit_prompt_with(text, steer, images);
    race_submit_and_stream(state, connection, session_id, submit, ui_rx).await
}

pub async fn drive_skill(
    state: &Arc<Mutex<AcpAgentState>>,
    connection: &ConnectionTo<Client>,
    session_id: &SessionId,
    session: Arc<crate::agent::CodingAgentSession>,
    name: String,
    args: String,
    ui_rx: &Arc<tokio::sync::Mutex<mpsc::UnboundedReceiver<AgentUiEvent>>>,
) -> anyhow::Result<()> {
    mark_running(state, session_id);
    send_running(connection, session_id)?;
    let submit = session.invoke_skill(&name, &args);
    race_submit_and_stream(state, connection, session_id, submit, ui_rx).await
}

pub async fn drive_template(
    state: &Arc<Mutex<AcpAgentState>>,
    connection: &ConnectionTo<Client>,
    session_id: &SessionId,
    session: Arc<crate::agent::CodingAgentSession>,
    name: String,
    args: String,
    ui_rx: &Arc<tokio::sync::Mutex<mpsc::UnboundedReceiver<AgentUiEvent>>>,
) -> anyhow::Result<()> {
    mark_running(state, session_id);
    send_running(connection, session_id)?;
    let submit = session.prompt_from_template(&name, &args);
    race_submit_and_stream(state, connection, session_id, submit, ui_rx).await
}

async fn race_submit_and_stream<F>(
    state: &Arc<Mutex<AcpAgentState>>,
    connection: &ConnectionTo<Client>,
    session_id: &SessionId,
    submit: F,
    ui_rx: &Arc<tokio::sync::Mutex<mpsc::UnboundedReceiver<AgentUiEvent>>>,
) -> anyhow::Result<()>
where
    F: std::future::Future<Output = anyhow::Result<()>>,
{
    let ids = {
        let guard = state.lock();
        guard
            .sessions
            .get(&session_key(session_id))
            .map(|s| s.ids.clone())
            .unwrap_or_default()
    };
    let mut ctx = StreamCtx {
        agent_msg: ids.next("msg_agent"),
        thought_msg: ids.next("msg_thought"),
        saw_text: false,
        agent_text: String::new(),
    };
    let mut rx = ui_rx.lock().await;
    while rx.try_recv().is_ok() {}

    tokio::pin!(submit);
    let mut submit_done = false;
    let mut submit_err = None;

    loop {
        if cancelled(state, session_id) {
            let _ = tools::cancel_open_tools(state, connection, session_id);
            mark_idle(state, session_id);
            let _ = send_idle(connection, session_id, StopReason::Cancelled);
            return Ok(());
        }
        tokio::select! {
            biased;
            event = rx.recv() => {
                let Some(event) = event else { break };
                apply_ui_event(state, connection, session_id, &mut ctx, event).await;
            }
            result = &mut submit, if !submit_done => {
                submit_done = true;
                if let Err(error) = result {
                    let _ = send_agent_text(connection, session_id, &format!("Prompt failed: {error:#}"));
                    submit_err = Some(error);
                }
            }
            _ = tokio::time::sleep(std::time::Duration::from_millis(400)), if submit_done => {
                break;
            }
        }
    }

    mark_idle(state, session_id);
    let _ = send_idle(connection, session_id, StopReason::EndTurn);
    match submit_err {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

struct StreamCtx {
    agent_msg: String,
    thought_msg: String,
    saw_text: bool,
    agent_text: String,
}

async fn apply_ui_event(
    state: &Arc<Mutex<AcpAgentState>>,
    connection: &ConnectionTo<Client>,
    session_id: &SessionId,
    ctx: &mut StreamCtx,
    event: AgentUiEvent,
) {
    match event {
        AgentUiEvent::TextDelta(text) if !text.is_empty() => {
            if !ctx.saw_text {
                let _ = send_running(connection, session_id);
                ctx.saw_text = true;
            }
            ctx.agent_text.push_str(&text);
            if let Err(error) = send_agent_chunk(connection, session_id, &ctx.agent_msg, text) {
                log::warn!("ACP agent chunk: {error:#}");
            }
        }
        AgentUiEvent::ThinkingDelta(text) if !text.is_empty() => {
            if let Err(error) = send_thought_chunk(connection, session_id, &ctx.thought_msg, text) {
                log::warn!("ACP thought chunk: {error:#}");
            }
        }
        AgentUiEvent::Retrying { .. } => {
            let _ = send_running(connection, session_id);
        }
        AgentUiEvent::Status(line) if !line.is_empty() => {
            let _ = send_agent_text(connection, session_id, &line);
        }
        AgentUiEvent::ToolStart {
            id, name, args_summary, ..
        } => {
            tools::track_tool_start(state, session_id, &id, &name);
            if let Err(error) = tools::on_tool_start(connection, session_id, &id, &name, &args_summary) {
                log::warn!("ACP tool start: {error:#}");
            }
            if super::terminals::is_local_shell_tool(&name)
                && let Err(error) = tools::on_shell_start(state, connection, session_id, &id, &args_summary)
            {
                log::warn!("ACP shell start: {error:#}");
            }
        }
        AgentUiEvent::ToolUpdate { id, output } => {
            if let Err(error) = tools::on_tool_update(state, connection, session_id, &id, &output) {
                log::warn!("ACP tool update: {error:#}");
            }
        }
        AgentUiEvent::ToolEnd {
            id,
            is_error,
            output,
            details,
        } => {
            if let Err(error) = tools::on_tool_end(state, connection, session_id, &id, is_error, &output, &details) {
                log::warn!("ACP tool end: {error:#}");
            }
            tools::track_tool_end(state, session_id, &id);
        }
        AgentUiEvent::TodoUpdated { items } => {
            if let Err(error) = plan::on_todos(connection, session_id, &items) {
                log::warn!("ACP plan update: {error:#}");
            }
        }
        AgentUiEvent::ToolApprovalRequired(req) => {
            let _ = send_requires_action(connection, session_id);
            let choice = permission::request_tool_approval(connection, session_id, &req).await;
            let _ = req.response_tx.send(choice);
            if !matches!(choice, ToolApprovalChoice::Reject) {
                let _ = send_running(connection, session_id);
            }
        }
        AgentUiEvent::PlanConfirmationRequired(req) => {
            let _ = send_requires_action(connection, session_id);
            let session = state
                .lock()
                .sessions
                .get(&session_key(session_id))
                .map(|s| Arc::clone(&s.session));
            if let Some(session) = session
                && let Err(error) = plan::confirm_plan(connection, session_id, &session, &req).await
            {
                log::warn!("ACP plan confirm: {error:#}");
            }
            let _ = send_running(connection, session_id);
        }
        AgentUiEvent::UserQuestionRequired(req) => {
            let _ = send_requires_action(connection, session_id);
            let prefer_form = state.lock().client_elicitation_form;
            if let Err(error) =
                crate::platform::acp::elicitation::ask_user(connection, session_id, req, prefer_form).await
            {
                log::warn!("ACP ask_user: {error:#}");
            }
            let _ = send_running(connection, session_id);
        }
        AgentUiEvent::ModeChangeRequired(req) => {
            let _ = send_requires_action(connection, session_id);
            let session = state
                .lock()
                .sessions
                .get(&session_key(session_id))
                .map(|s| Arc::clone(&s.session));
            if let Some(session) = session {
                if let Err(error) = permission::request_mode_change(connection, session_id, &session, req).await {
                    log::warn!("ACP mode change: {error:#}");
                }
            } else {
                let _ = req.response_tx.send("false".into());
            }
            let _ = send_running(connection, session_id);
        }
        AgentUiEvent::RunCompleted { usage, .. } => {
            if !ctx.agent_text.is_empty() {
                let _ = send_update(
                    connection,
                    session_id,
                    SessionUpdate::AgentMessage(
                        AgentMessage::new(ctx.agent_msg.clone())
                            .content(vec![ContentBlock::Text(TextContent::new(ctx.agent_text.clone()))]),
                    ),
                );
                ctx.agent_text.clear();
            }
            if let Some(usage) = usage {
                let used = usage.input_tokens.saturating_add(usage.output_tokens).max(0) as u64;
                let _ = send_update(
                    connection,
                    session_id,
                    SessionUpdate::UsageUpdate(UsageUpdate::new(used, used.max(1))),
                );
            }
            let _ = send_running(connection, session_id);
        }
        AgentUiEvent::AsideFinished { answer, question, .. } => {
            let _ = send_agent_text(connection, session_id, &format!("/aside {question}\n\n{answer}"));
        }
        AgentUiEvent::AsideFailed { error, .. } => {
            let _ = send_agent_text(connection, session_id, &format!("/aside error: {error}"));
        }
        _ => {}
    }
}

fn cancelled(state: &Arc<Mutex<AcpAgentState>>, session_id: &SessionId) -> bool {
    state
        .lock()
        .sessions
        .get(&session_key(session_id))
        .map(|s| s.cancelled.load(Ordering::Relaxed))
        .unwrap_or(false)
}

fn mark_idle(state: &Arc<Mutex<AcpAgentState>>, session_id: &SessionId) {
    if let Some(s) = state.lock().sessions.get(&session_key(session_id)) {
        s.running.store(false, Ordering::Relaxed);
    }
}

pub fn mark_running(state: &Arc<Mutex<AcpAgentState>>, session_id: &SessionId) {
    if let Some(s) = state.lock().sessions.get(&session_key(session_id)) {
        s.running.store(true, Ordering::Relaxed);
        s.cancelled.store(false, Ordering::Relaxed);
    }
}

pub fn is_running(state: &Arc<Mutex<AcpAgentState>>, session_id: &SessionId) -> bool {
    state
        .lock()
        .sessions
        .get(&session_key(session_id))
        .map(|s| s.running.load(Ordering::Relaxed))
        .unwrap_or(false)
}
