//! Agent event processing.

use std::sync::Arc;

use elph_ai::Message;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use crate::types::{AgentEvent, AgentMessage};

use super::AgentListener;
use super::state::MutableAgentState;

pub(super) async fn process_event(
    state: &Arc<Mutex<MutableAgentState>>,
    event: &AgentEvent,
    listeners: &Arc<Mutex<Vec<AgentListener>>>,
    token: &CancellationToken,
) {
    {
        let mut state = state.lock().await;
        match event {
            AgentEvent::MessageStart { message } => {
                state.set_streaming_message(Some(message.clone()));
            }
            AgentEvent::MessageUpdate { message, .. } => {
                state.set_streaming_message(Some(message.clone()));
            }
            AgentEvent::MessageEnd { message } => {
                state.set_streaming_message(None);
                state.push_message(message.clone());
            }
            AgentEvent::ToolExecutionStart { tool_call_id, .. } => {
                state.add_pending_tool_call(tool_call_id.clone());
            }
            AgentEvent::ToolExecutionEnd { tool_call_id, .. } => {
                state.remove_pending_tool_call(tool_call_id);
            }
            AgentEvent::TurnEnd { message, .. } => {
                if let AgentMessage::Llm(message) = message
                    && let Message::Assistant(assistant) = message.as_ref()
                    && assistant.error_message.is_some()
                {
                    state.set_error_message(assistant.error_message.clone());
                }
            }
            AgentEvent::AgentEnd { .. } => {
                state.set_streaming_message(None);
            }
            _ => {}
        }
    }

    let listeners = listeners.lock().await.clone();
    for listener in &listeners {
        listener(event.clone(), token.clone()).await;
    }
}

pub(super) fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
