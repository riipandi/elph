//! Tool execution pipeline for the agent loop.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use elph_ai::validation::validate_tool_call;
use elph_ai::{AssistantMessage, Message, ToolCall};
use tokio_util::sync::CancellationToken;

use crate::agent_loop::AgentEventCallback;
use crate::types::{
    AfterToolCallContext, AfterToolCallResult, AgentContext, AgentEvent, AgentLoopConfig, AgentTool, AgentToolResult,
    BeforeToolCallContext, ToolExecutionMode, ToolResultContent, tool_result_to_agent,
};

pub struct ExecutedToolBatch {
    pub messages: Vec<Message>,
    pub terminate: bool,
}

pub async fn fail_tool_calls_from_truncated_message(
    tool_calls: &[ToolCall],
    emit: &AgentEventCallback,
) -> ExecutedToolBatch {
    let mut messages = Vec::new();
    for tool_call in tool_calls {
        emit(AgentEvent::ToolExecutionStart {
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
        execute_tool_calls_sequential(current_context, assistant_message, tool_calls, config, signal, emit).await
    } else {
        execute_tool_calls_parallel(current_context, assistant_message, tool_calls, config, signal, emit).await
    }
}

async fn execute_tool_calls_sequential(
    current_context: &AgentContext,
    assistant_message: &AssistantMessage,
    tool_calls: &[ToolCall],
    config: &AgentLoopConfig,
    signal: Option<CancellationToken>,
    emit: &AgentEventCallback,
) -> ExecutedToolBatch {
    let mut finalized_calls = Vec::new();
    let mut messages = Vec::new();

    for tool_call in tool_calls {
        emit(AgentEvent::ToolExecutionStart {
            tool_call_id: tool_call.id.clone(),
            tool_name: tool_call.name.clone(),
            args: tool_call.arguments.clone(),
        })
        .await;

        let preparation =
            prepare_tool_call(current_context, assistant_message, tool_call, config, signal.clone()).await;
        let finalized = match preparation {
            Preparation::Immediate { result, is_error } => FinalizedToolCall {
                tool_call: tool_call.clone(),
                result,
                is_error,
            },
            Preparation::Prepared(prepared) => {
                let executed = execute_prepared_tool_call(prepared.as_ref(), signal.clone(), emit).await;
                finalize_executed_tool_call(
                    current_context,
                    assistant_message,
                    prepared.as_ref(),
                    executed,
                    config,
                    signal.clone(),
                )
                .await
            }
        };

        emit_tool_execution_end(&finalized, emit).await;
        let tool_result = create_tool_result_message(&finalized);
        emit_tool_result_message(&tool_result, emit).await;
        finalized_calls.push(finalized);
        messages.push(tool_result);

        if signal.as_ref().is_some_and(|t| t.is_cancelled()) {
            break;
        }
    }

    ExecutedToolBatch {
        messages,
        terminate: should_terminate_tool_batch(&finalized_calls),
    }
}

async fn execute_tool_calls_parallel(
    current_context: &AgentContext,
    assistant_message: &AssistantMessage,
    tool_calls: &[ToolCall],
    config: &AgentLoopConfig,
    signal: Option<CancellationToken>,
    emit: &AgentEventCallback,
) -> ExecutedToolBatch {
    enum Entry {
        Immediate(FinalizedToolCall),
        Deferred(Pin<Box<dyn Future<Output = FinalizedToolCall> + Send>>),
    }

    let mut entries: Vec<Entry> = Vec::new();

    for tool_call in tool_calls {
        emit(AgentEvent::ToolExecutionStart {
            tool_call_id: tool_call.id.clone(),
            tool_name: tool_call.name.clone(),
            args: tool_call.arguments.clone(),
        })
        .await;

        let preparation =
            prepare_tool_call(current_context, assistant_message, tool_call, config, signal.clone()).await;
        match preparation {
            Preparation::Immediate { result, is_error } => {
                let finalized = FinalizedToolCall {
                    tool_call: tool_call.clone(),
                    result,
                    is_error,
                };
                emit_tool_execution_end(&finalized, emit).await;
                entries.push(Entry::Immediate(finalized));
            }
            Preparation::Prepared(prepared) => {
                let emit = emit.clone();
                let signal = signal.clone();
                let current_context = current_context.clone();
                let assistant_message = assistant_message.clone();
                let config_hooks = config.after_tool_call.clone();
                entries.push(Entry::Deferred(Box::pin(async move {
                    let executed = execute_prepared_tool_call(prepared.as_ref(), signal.clone(), &emit).await;
                    finalize_executed_tool_call_with_hook(
                        &current_context,
                        &assistant_message,
                        prepared.as_ref(),
                        executed,
                        config_hooks,
                        signal,
                    )
                    .await
                })));
            }
        }

        if signal.as_ref().is_some_and(|t| t.is_cancelled()) {
            break;
        }
    }

    let mut finalized_calls = Vec::new();
    for entry in entries {
        let finalized = match entry {
            Entry::Immediate(f) => f,
            Entry::Deferred(fut) => {
                let f = fut.await;
                emit_tool_execution_end(&f, emit).await;
                f
            }
        };
        finalized_calls.push(finalized);
    }

    let mut messages = Vec::new();
    for finalized in &finalized_calls {
        let tool_result = create_tool_result_message(finalized);
        emit_tool_result_message(&tool_result, emit).await;
        messages.push(tool_result);
    }

    ExecutedToolBatch {
        messages,
        terminate: should_terminate_tool_batch(&finalized_calls),
    }
}

struct PreparedToolCall {
    tool_call: ToolCall,
    tool: AgentTool,
    args: serde_json::Value,
}

enum Preparation {
    Prepared(Box<PreparedToolCall>),
    Immediate { result: AgentToolResult, is_error: bool },
}

struct FinalizedToolCall {
    tool_call: ToolCall,
    result: AgentToolResult,
    is_error: bool,
}

struct ExecutedToolCallOutcome {
    result: AgentToolResult,
    is_error: bool,
}

fn should_terminate_tool_batch(finalized: &[FinalizedToolCall]) -> bool {
    !finalized.is_empty() && finalized.iter().all(|f| f.result.terminate == Some(true))
}

fn prepare_tool_call_arguments(tool: &AgentTool, tool_call: &ToolCall) -> ToolCall {
    if let Some(prepare) = &tool.prepare_arguments {
        let prepared = prepare(tool_call.arguments.clone());
        if prepared == tool_call.arguments {
            return tool_call.clone();
        }
        let mut tc = tool_call.clone();
        tc.arguments = prepared;
        tc
    } else {
        tool_call.clone()
    }
}

async fn prepare_tool_call(
    current_context: &AgentContext,
    assistant_message: &AssistantMessage,
    tool_call: &ToolCall,
    config: &AgentLoopConfig,
    signal: Option<CancellationToken>,
) -> Preparation {
    let Some(tool) = current_context.tools.iter().find(|t| t.name() == tool_call.name) else {
        return Preparation::Immediate {
            result: AgentToolResult::error(format!("Tool {} not found", tool_call.name)),
            is_error: true,
        };
    };

    let prepared_tool_call = prepare_tool_call_arguments(tool, tool_call);

    let validated_args = match validate_tool_call(&tool.tool, &prepared_tool_call) {
        Ok(()) => prepared_tool_call.arguments.clone(),
        Err(msg) => {
            return Preparation::Immediate {
                result: AgentToolResult::error(msg),
                is_error: true,
            };
        }
    };

    if let Some(before) = &config.before_tool_call {
        let before_result = before(
            BeforeToolCallContext {
                assistant_message: assistant_message.clone(),
                tool_call: tool_call.clone(),
                args: validated_args.clone(),
                context: current_context.clone(),
            },
            signal.clone(),
        )
        .await;
        if signal.as_ref().is_some_and(|t| t.is_cancelled()) {
            return Preparation::Immediate {
                result: AgentToolResult::error("Operation aborted"),
                is_error: true,
            };
        }
        if let Some(result) = before_result
            && result.block
        {
            return Preparation::Immediate {
                result: AgentToolResult::error(
                    result
                        .reason
                        .unwrap_or_else(|| "Tool execution was blocked".to_string()),
                ),
                is_error: true,
            };
        }
    }

    if signal.as_ref().is_some_and(|t| t.is_cancelled()) {
        return Preparation::Immediate {
            result: AgentToolResult::error("Operation aborted"),
            is_error: true,
        };
    }

    Preparation::Prepared(Box::new(PreparedToolCall {
        tool_call: tool_call.clone(),
        tool: tool.clone(),
        args: validated_args,
    }))
}

async fn execute_prepared_tool_call(
    prepared: &PreparedToolCall,
    signal: Option<CancellationToken>,
    emit: &AgentEventCallback,
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
                    emit(AgentEvent::ToolExecutionUpdate {
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

async fn finalize_executed_tool_call(
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

async fn finalize_executed_tool_call_with_hook(
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

async fn emit_tool_execution_end(finalized: &FinalizedToolCall, emit: &AgentEventCallback) {
    emit(AgentEvent::ToolExecutionEnd {
        tool_call_id: finalized.tool_call.id.clone(),
        tool_name: finalized.tool_call.name.clone(),
        result: finalized.result.clone(),
        is_error: finalized.is_error,
    })
    .await;
}

fn create_tool_result_message(finalized: &FinalizedToolCall) -> Message {
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

async fn emit_tool_result_message(tool_result: &Message, emit: &AgentEventCallback) {
    let agent_msg = tool_result_to_agent(tool_result.clone());
    emit(AgentEvent::MessageStart {
        message: agent_msg.clone(),
    })
    .await;
    emit(AgentEvent::MessageEnd { message: agent_msg }).await;
}
