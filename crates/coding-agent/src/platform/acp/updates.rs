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
use crate::platform::acp::limits::truncate_text;
use crate::platform::acp::permission;
use crate::platform::acp::plan;
use crate::platform::acp::state::MessageIds;
use crate::platform::acp::state::{AcpAgentState, session_cancel_notify, session_key, session_stream_gate};
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
    let submit = async move { session.submit_prompt_with(text, steer, images).await };
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
    let submit = async move { session.invoke_skill(&name, &args).await };
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
    let submit = async move { session.prompt_from_template(&name, &args).await };
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
    F: std::future::Future<Output = anyhow::Result<()>> + Send + 'static,
{
    let key = session_key(session_id);
    let cancel = session_cancel_notify(state, &key);
    let gate = session_stream_gate(state, &key);
    let submit = tokio::spawn(submit);

    let Some(gate) = gate else {
        return submit.await.unwrap_or_else(|e| Err(anyhow::anyhow!("{e}")));
    };
    let Ok(_stream_permit) = gate.try_lock() else {
        // Another turn owns the UI stream; this submit (steer) still runs.
        return submit.await.unwrap_or_else(|e| Err(anyhow::anyhow!("{e}")));
    };

    let ids = {
        let guard = state.lock();
        guard.sessions.get(&key).map(|s| s.ids.clone()).unwrap_or_default()
    };
    let mut ctx = StreamCtx {
        ids,
        agent_msg: String::new(),
        thought_msg: String::new(),
        saw_text: false,
        agent_text: String::new(),
    };
    ctx.rotate_messages();

    let mut submit = submit;
    let mut submit_done = false;
    let mut submit_err = None;
    let mut pending: std::collections::VecDeque<AgentUiEvent> = std::collections::VecDeque::new();

    loop {
        if cancelled(state, session_id) {
            let _ = tools::cancel_open_tools(state, connection, session_id);
            mark_idle(state, session_id);
            let _ = send_idle(connection, session_id, StopReason::Cancelled);
            return Ok(());
        }

        let next = if let Some(event) = pending.pop_front() {
            Some(event)
        } else {
            let mut rx = ui_rx.lock().await;
            drain_stale(&mut rx, &mut pending);
            if let Some(event) = pending.pop_front() {
                Some(event)
            } else {
                tokio::select! {
                    biased;
                    event = rx.recv() => event,
                    result = &mut submit, if !submit_done => {
                        submit_done = true;
                        match result {
                            Ok(Ok(())) => {}
                            Ok(Err(error)) => {
                                let _ = send_agent_text(connection, session_id, &format!("Prompt failed: {error:#}"));
                                submit_err = Some(error);
                            }
                            Err(error) => submit_err = Some(anyhow::anyhow!("{error}")),
                        }
                        None
                    }
                    _ = tokio::time::sleep(std::time::Duration::from_millis(400)), if submit_done => {
                        break;
                    }
                    _ = async {
                        if let Some(cancel) = &cancel {
                            cancel.notified().await;
                        } else {
                            std::future::pending::<()>().await;
                        }
                    } => {
                        let _ = tools::cancel_open_tools(state, connection, session_id);
                        mark_idle(state, session_id);
                        let _ = send_idle(connection, session_id, StopReason::Cancelled);
                        return Ok(());
                    }
                }
            }
        };

        if let Some(event) = next {
            apply_ui_event(state, connection, session_id, &mut ctx, event, cancel.clone()).await;
        }
    }

    mark_idle(state, session_id);
    let _ = send_idle(connection, session_id, StopReason::EndTurn);
    match submit_err {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

pub(crate) fn is_interactive_event(event: &AgentUiEvent) -> bool {
    matches!(
        event,
        AgentUiEvent::ToolApprovalRequired(_)
            | AgentUiEvent::PlanConfirmationRequired(_)
            | AgentUiEvent::UserQuestionRequired(_)
            | AgentUiEvent::ModeChangeRequired(_)
    )
}

pub(crate) fn drain_stale(
    rx: &mut mpsc::UnboundedReceiver<AgentUiEvent>,
    pending: &mut std::collections::VecDeque<AgentUiEvent>,
) {
    while let Ok(event) = rx.try_recv() {
        if is_interactive_event(&event) {
            pending.push_back(event);
        }
    }
}

struct StreamCtx {
    ids: MessageIds,
    agent_msg: String,
    thought_msg: String,
    saw_text: bool,
    agent_text: String,
}

impl StreamCtx {
    fn rotate_messages(&mut self) {
        self.agent_msg = self.ids.next("msg_agent");
        self.thought_msg = self.ids.next("msg_thought");
        self.saw_text = false;
        self.agent_text.clear();
    }
}

async fn apply_ui_event(
    state: &Arc<Mutex<AcpAgentState>>,
    connection: &ConnectionTo<Client>,
    session_id: &SessionId,
    ctx: &mut StreamCtx,
    event: AgentUiEvent,
    cancel: Option<Arc<tokio::sync::Notify>>,
) {
    match event {
        AgentUiEvent::TextDelta(text) if !text.is_empty() => {
            if !ctx.saw_text {
                let _ = send_running(connection, session_id);
                ctx.saw_text = true;
            }
            ctx.agent_text.push_str(&text);
            if let Err(error) = send_agent_chunk(connection, session_id, &ctx.agent_msg, truncate_text(&text)) {
                log::warn!("ACP agent chunk: {error:#}");
            }
        }
        AgentUiEvent::ThinkingDelta(text) if !text.is_empty() => {
            if let Err(error) = send_thought_chunk(connection, session_id, &ctx.thought_msg, truncate_text(&text)) {
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
            let choice = permission::request_tool_approval(connection, session_id, &req, cancel).await;
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
                && let Err(error) = plan::confirm_plan(connection, session_id, &session, &req, cancel).await
            {
                log::warn!("ACP plan confirm: {error:#}");
            }
            let _ = send_running(connection, session_id);
        }
        AgentUiEvent::UserQuestionRequired(req) => {
            let _ = send_requires_action(connection, session_id);
            let prefer_form = state.lock().client_elicitation_form;
            if let Err(error) =
                crate::platform::acp::elicitation::ask_user(connection, session_id, req, prefer_form, cancel).await
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
                if let Err(error) = permission::request_mode_change(connection, session_id, &session, req, cancel).await
                {
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
            ctx.rotate_messages();
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

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;

    #[test]
    fn interactive_events_are_kept() {
        assert!(is_interactive_event(&AgentUiEvent::ToolApprovalRequired(
            crate::agent::ToolApprovalRequest {
                tool_call_id: "t".into(),
                tool_name: "read_file".into(),
                args_summary: String::new(),
                response_tx: tokio::sync::oneshot::channel().0,
            }
        )));
        assert!(!is_interactive_event(&AgentUiEvent::TextDelta("x".into())));
        assert!(!is_interactive_event(&AgentUiEvent::RunCompleted {
            elapsed_secs: 0.0,
            usage: None,
            provider_id: None,
            model_id: None,
        }));
    }

    #[test]
    fn drain_keeps_approval_drops_text() {
        let (tx, rx) = mpsc::unbounded_channel();
        let _ = tx.send(AgentUiEvent::TextDelta("gone".into()));
        let (response_tx, _rx) = tokio::sync::oneshot::channel();
        let _ = tx.send(AgentUiEvent::ToolApprovalRequired(crate::agent::ToolApprovalRequest {
            tool_call_id: "t1".into(),
            tool_name: "shell_exec".into(),
            args_summary: "ls".into(),
            response_tx,
        }));
        let _ = tx.send(AgentUiEvent::Status("nope".into()));
        drop(tx);
        let mut rx = rx;
        let mut pending = std::collections::VecDeque::new();
        drain_stale(&mut rx, &mut pending);
        assert_eq!(pending.len(), 1);
        assert!(matches!(pending[0], AgentUiEvent::ToolApprovalRequired(_)));
    }
}
