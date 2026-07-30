//! Tool result message construction and event emission.

use elph_ai::Message;

use super::super::AgentEventCallback;
use crate::types::tool_result_to_agent;
use crate::types::{AgentEvent, ToolResultContent};

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

    // Persist wall-clock duration for transcript resume (TUI reads `_elph_ui.duration_secs`).
    let mut details = finalized.result.details.clone();
    if let Some(secs) = finalized.duration_secs {
        merge_elph_ui_duration(&mut details, secs);
    }

    Message::ToolResult {
        tool_call_id: finalized.tool_call.id.clone(),
        tool_name: finalized.tool_call.name.clone(),
        content,
        details: Some(details),
        added_tool_names: finalized.result.added_tool_names.clone(),
        usage: finalized.result.usage.clone().map(|b| *b),
        is_error: finalized.is_error,
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0),
    }
}

fn merge_elph_ui_duration(details: &mut serde_json::Value, duration_secs: f64) {
    if !duration_secs.is_finite() || duration_secs < 0.0 {
        return;
    }
    if !details.is_object() {
        *details = serde_json::json!({});
    }
    let Some(obj) = details.as_object_mut() else {
        return;
    };
    let ui = obj
        .entry("_elph_ui".to_string())
        .or_insert_with(|| serde_json::json!({}));
    if let Some(ui_obj) = ui.as_object_mut() {
        ui_obj.insert("duration_secs".into(), serde_json::json!(duration_secs));
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
