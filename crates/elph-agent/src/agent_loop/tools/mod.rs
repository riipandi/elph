//! Tool execution pipeline for the agent loop.

mod dispatch;
mod execute;
mod messages;
mod prepare;

use elph_ai::{AssistantMessage, ToolCall};
use tokio_util::sync::CancellationToken;

use crate::agent_loop::AgentEventCallback;
use crate::types::{AgentContext, AgentLoopConfig, AgentToolResult, ToolExecutionMode};

pub struct ExecutedToolBatch {
    pub messages: Vec<elph_ai::Message>,
    pub terminate: bool,
}

pub(super) struct PreparedToolCall {
    pub tool_call: ToolCall,
    pub tool: crate::types::AgentTool,
    pub args: serde_json::Value,
}

pub(super) enum Preparation {
    Prepared(Box<PreparedToolCall>),
    Immediate { result: AgentToolResult, is_error: bool },
}

pub(super) struct FinalizedToolCall {
    pub tool_call: ToolCall,
    pub result: AgentToolResult,
    pub is_error: bool,
}

pub(super) struct ExecutedToolCallOutcome {
    pub result: AgentToolResult,
    pub is_error: bool,
}

pub(super) fn should_terminate_tool_batch(finalized: &[FinalizedToolCall]) -> bool {
    !finalized.is_empty() && finalized.iter().all(|f| f.result.terminate == Some(true))
}

pub async fn fail_tool_calls_from_truncated_message(
    tool_calls: &[ToolCall],
    emit: &AgentEventCallback,
) -> ExecutedToolBatch {
    use messages::{create_tool_result_message, emit_tool_execution_end, emit_tool_result_message};

    let mut messages = Vec::new();
    for tool_call in tool_calls {
        emit(crate::types::AgentEvent::ToolExecutionStart {
            tool_call_id: tool_call.id.clone(),
            tool_name: tool_call.name.clone(),
            args: tool_call.arguments.clone(),
        })
        .await;

        let result = AgentToolResult::error(format!(
            "Tool call \"{}\" was not executed: the response hit the output token limit, so its arguments may be truncated. Re-issue the tool call with complete arguments.",
            tool_call.name
        ));
        let finalized = FinalizedToolCall {
            tool_call: tool_call.clone(),
            result: result.clone(),
            is_error: true,
        };
        emit_tool_execution_end(&finalized, emit).await;
        let tool_result = create_tool_result_message(&finalized);
        emit_tool_result_message(&tool_result, emit).await;
        messages.push(tool_result);
    }
    ExecutedToolBatch {
        messages,
        terminate: false,
    }
}

pub async fn execute_tool_calls(
    current_context: &AgentContext,
    assistant_message: &AssistantMessage,
    tool_calls: &[ToolCall],
    config: &AgentLoopConfig,
    signal: Option<CancellationToken>,
    emit: &AgentEventCallback,
) -> ExecutedToolBatch {
    let has_sequential = tool_calls.iter().any(|tc| {
        current_context
            .tools
            .iter()
            .find(|t| t.name() == tc.name)
            .and_then(|t| t.execution_mode)
            == Some(ToolExecutionMode::Sequential)
    });

    if config.tool_execution == ToolExecutionMode::Sequential || has_sequential {
        dispatch::execute_tool_calls_sequential(current_context, assistant_message, tool_calls, config, signal, emit)
            .await
    } else {
        dispatch::execute_tool_calls_parallel(current_context, assistant_message, tool_calls, config, signal, emit)
            .await
    }
}
