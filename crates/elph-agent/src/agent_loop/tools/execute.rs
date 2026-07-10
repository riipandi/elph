//! Tool execution and post-execution finalization.

use std::sync::Arc;

use elph_ai::AssistantMessage;
use tokio_util::sync::CancellationToken;

use crate::types::{AfterToolCallContext, AfterToolCallResult, AgentContext, AgentLoopConfig, AgentToolResult};

use super::{ExecutedToolCallOutcome, FinalizedToolCall, PreparedToolCall};

pub(super) async fn execute_prepared_tool_call(
    prepared: &PreparedToolCall,
    signal: Option<CancellationToken>,
    emit: &crate::agent_loop::AgentEventCallback,
) -> ExecutedToolCallOutcome {
    let update_tx: Arc<tokio::sync::Mutex<bool>> = Arc::new(tokio::sync::Mutex::new(true));
    let on_update = {
        let emit = emit.clone();
        let tool_call = prepared.tool_call.clone();
        let accepting = update_tx.clone();
        Arc::new(move |partial: AgentToolResult| {
            let emit = emit.clone();
            let tool_call = tool_call.clone();
            let accepting = accepting.clone();
            tokio::spawn(async move {
                if *accepting.lock().await {
                    emit(crate::types::AgentEvent::ToolExecutionUpdate {
                        tool_call_id: tool_call.id.clone(),
                        tool_name: tool_call.name.clone(),
                        args: tool_call.arguments.clone(),
                        partial_result: partial,
                    })
                    .await;
                }
            });
        }) as crate::types::ToolUpdateCallback
    };

    match (prepared.tool.execute)(
        prepared.tool_call.id.clone(),
        prepared.args.clone(),
        signal,
        Some(on_update),
    )
    .await
    {
        Ok(result) => {
            *update_tx.lock().await = false;
            ExecutedToolCallOutcome {
                result,
                is_error: false,
            }
        }
        Err(error) => {
            *update_tx.lock().await = false;
            ExecutedToolCallOutcome {
                result: AgentToolResult::error(error.to_string()),
                is_error: true,
            }
        }
    }
}

pub(super) async fn finalize_executed_tool_call(
    current_context: &AgentContext,
    assistant_message: &AssistantMessage,
    prepared: &PreparedToolCall,
    executed: ExecutedToolCallOutcome,
    config: &AgentLoopConfig,
    signal: Option<CancellationToken>,
) -> FinalizedToolCall {
    finalize_executed_tool_call_with_hook(
        current_context,
        assistant_message,
        prepared,
        executed,
        config.after_tool_call.clone(),
        signal,
    )
    .await
}

pub(super) async fn finalize_executed_tool_call_with_hook(
    current_context: &AgentContext,
    assistant_message: &AssistantMessage,
    prepared: &PreparedToolCall,
    mut executed: ExecutedToolCallOutcome,
    after_hook: Option<crate::types::AfterToolCallFn>,
    signal: Option<CancellationToken>,
) -> FinalizedToolCall {
    if let Some(after) = after_hook
        && let Some(after_result) = after(
            AfterToolCallContext {
                assistant_message: assistant_message.clone(),
                tool_call: prepared.tool_call.clone(),
                args: prepared.args.clone(),
                result: executed.result.clone(),
                is_error: executed.is_error,
                context: current_context.clone(),
            },
            signal,
        )
        .await
    {
        apply_after_tool_call(&mut executed, after_result);
    }

    FinalizedToolCall {
        tool_call: prepared.tool_call.clone(),
        result: executed.result,
        is_error: executed.is_error,
    }
}

fn apply_after_tool_call(executed: &mut ExecutedToolCallOutcome, after: AfterToolCallResult) {
    if let Some(content) = after.content {
        executed.result.content = content;
    }
    if let Some(details) = after.details {
        executed.result.details = details;
    }
    if let Some(is_error) = after.is_error {
        executed.is_error = is_error;
    }
    if let Some(terminate) = after.terminate {
        executed.result.terminate = Some(terminate);
    }
}
