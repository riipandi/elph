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
pub async fn drive_turn(
    state: &Arc<Mutex<AcpAgentState>>,
    connection: &ConnectionTo<Client>,
    session_id: &SessionId,
    session: Arc<crate::agent::CodingAgentSession>,
    text: String,
    steer: bool,
    ui_rx: &Arc<tokio::sync::Mutex<mpsc::UnboundedReceiver<AgentUiEvent>>>,
) -> anyhow::Result<()> {
    mark_running(state, session_id);
    send_running(connection, session_id)?;
    let submit = session.submit_prompt(text, steer);
    tokio::pin!(submit);
    let stream = stream_ui_events(state, connection, session_id, ui_rx);
    tokio::pin!(stream);
    tokio::select! {
        submit_res = &mut submit => match submit_res {
            Ok(()) => stream.await,
            Err(error) => {
                let _ = send_agent_text(connection, session_id, &format!("Prompt failed: {error:#}"));
                mark_idle(state, session_id);
                send_idle(connection, session_id, StopReason::EndTurn)?;
                Err(error)
            }
        },
        stream_res = &mut stream => {
            let _ = submit.await;
            stream_res
        }
    }
}

pub async fn stream_ui_events(
    state: &Arc<Mutex<AcpAgentState>>,
    connection: &ConnectionTo<Client>,
    session_id: &SessionId,
    ui_rx: &Arc<tokio::sync::Mutex<mpsc::UnboundedReceiver<AgentUiEvent>>>,
) -> anyhow::Result<()> {
    let ids = {
        let guard = state.lock();
        guard
            .sessions
            .get(&session_key(session_id))
            .map(|s| s.ids.clone())
            .unwrap_or_default()
    };
    let agent_msg = ids.next("msg_agent");
    let thought_msg = ids.next("msg_thought");
    let mut saw_text = false;
    let mut agent_text = String::new();
    let mut rx = ui_rx.lock().await;

    while let Some(event) = rx.recv().await {
        if cancelled(state, session_id) {
            tools::cancel_open_tools(state, connection, session_id)?;
            send_idle(connection, session_id, StopReason::Cancelled)?;
            return Ok(());
        }

        match event {
            AgentUiEvent::TextDelta(text) if !text.is_empty() => {
                if !saw_text {
                    send_running(connection, session_id)?;
                    saw_text = true;
                }
                agent_text.push_str(&text);
                send_agent_chunk(connection, session_id, &agent_msg, text)?;
            }
            AgentUiEvent::ThinkingDelta(text) if !text.is_empty() => {
                send_thought_chunk(connection, session_id, &thought_msg, text)?;
            }
            AgentUiEvent::ToolStart {
                id, name, args_summary, ..
            } => {
                tools::track_tool_start(state, session_id, &id);
                tools::on_tool_start(connection, session_id, &id, &name, &args_summary)?;
            }
            AgentUiEvent::ToolUpdate { id, output } => {
                tools::on_tool_update(connection, session_id, &id, &output)?;
            }
            AgentUiEvent::ToolEnd {
                id,
                is_error,
                output,
                details,
            } => {
                tools::track_tool_end(state, session_id, &id);
                tools::on_tool_end(connection, session_id, &id, is_error, &output, &details)?;
            }
            AgentUiEvent::TodoUpdated { items } => {
                plan::on_todos(connection, session_id, &items)?;
            }
            AgentUiEvent::ToolApprovalRequired(req) => {
                send_requires_action(connection, session_id)?;
                let choice = permission::request_tool_approval(connection, session_id, &req).await;
                let _ = req.response_tx.send(choice);
                if !matches!(choice, ToolApprovalChoice::Reject) {
                    send_running(connection, session_id)?;
                }
            }
            AgentUiEvent::PlanConfirmationRequired(req) => {
                send_requires_action(connection, session_id)?;
                plan::confirm_plan(connection, session_id, &req).await?;
                send_running(connection, session_id)?;
            }
            AgentUiEvent::UserQuestionRequired(req) => {
                send_requires_action(connection, session_id)?;
                crate::platform::acp::elicitation::ask_user(connection, session_id, req).await?;
                send_running(connection, session_id)?;
            }
            AgentUiEvent::ModeChangeRequired(req) => {
                send_requires_action(connection, session_id)?;
                permission::request_mode_change(connection, session_id, req).await?;
                send_running(connection, session_id)?;
            }
            AgentUiEvent::RunCompleted { usage, .. } => {
                if !agent_text.is_empty() {
                    let _ = send_update(
                        connection,
                        session_id,
                        SessionUpdate::AgentMessage(
                            AgentMessage::new(agent_msg.clone())
                                .content(vec![ContentBlock::Text(TextContent::new(agent_text.clone()))]),
                        ),
                    );
                }
                if let Some(usage) = usage {
                    let used = usage.input_tokens.saturating_add(usage.output_tokens).max(0) as u64;
                    let _ = send_update(
                        connection,
                        session_id,
                        SessionUpdate::UsageUpdate(UsageUpdate::new(used, used.max(1))),
                    );
                }
                mark_idle(state, session_id);
                send_idle(connection, session_id, StopReason::EndTurn)?;
                return Ok(());
            }
            AgentUiEvent::AsideFinished { answer, question, .. } => {
                send_agent_text(connection, session_id, &format!("/aside {question}\n\n{answer}"))?;
            }
            AgentUiEvent::AsideFailed { error, .. } => {
                send_agent_text(connection, session_id, &format!("/aside error: {error}"))?;
            }
            _ => {}
        }
        let _ = (&agent_msg, &thought_msg);
    }

    mark_idle(state, session_id);
    send_idle(connection, session_id, StopReason::EndTurn)?;
    Ok(())
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
