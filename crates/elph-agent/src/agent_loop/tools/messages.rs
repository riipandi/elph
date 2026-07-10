//! Tool result message construction and event emission.

use elph_ai::Message;

use crate::agent_loop::AgentEventCallback;
use crate::types::{AgentEvent, ToolResultContent, tool_result_to_agent};

use super::FinalizedToolCall;

pub(super) async fn emit_tool_execution_end(finalized: &FinalizedToolCall, emit: &AgentEventCallback) {
    emit(AgentEvent::ToolExecutionEnd {
        tool_call_id: finalized.tool_call.id.clone(),
        tool_name: finalized.tool_call.name.clone(),
        result: finalized.result.clone(),
        is_error: finalized.is_error,
    })
    .await;
}

pub(super) fn create_tool_result_message(finalized: &FinalizedToolCall) -> Message {
    let content: Vec<elph_ai::ContentBlock> = finalized
        .result
        .content
        .iter()
        .map(|c| match c {
            ToolResultContent::Text(t) => elph_ai::ContentBlock::Text { text: t.text.clone() },
            ToolResultContent::Image(i) => elph_ai::ContentBlock::Image {
                data: i.data.clone(),
                mime_type: i.mime_type.clone(),
            },
        })
        .collect();

    Message::ToolResult {
        tool_call_id: finalized.tool_call.id.clone(),
        tool_name: finalized.tool_call.name.clone(),
        content,
        details: Some(finalized.result.details.clone()),
        is_error: finalized.is_error,
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0),
    }
}

pub(super) async fn emit_tool_result_message(tool_result: &Message, emit: &AgentEventCallback) {
    let agent_msg = tool_result_to_agent(tool_result.clone());
    emit(AgentEvent::MessageStart {
        message: agent_msg.clone(),
    })
    .await;
    emit(AgentEvent::MessageEnd { message: agent_msg }).await;
}
