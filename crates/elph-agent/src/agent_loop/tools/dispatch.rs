//! Sequential and parallel tool call batch execution.

use std::future::Future;
use std::pin::Pin;

use elph_ai::{AssistantMessage, ToolCall};
use tokio_util::sync::CancellationToken;

use crate::agent_loop::AgentEventCallback;
use crate::types::{AgentContext, AgentEvent, AgentLoopConfig};

use super::execute::{execute_prepared_tool_call, finalize_executed_tool_call, finalize_executed_tool_call_with_hook};
use super::messages::{create_tool_result_message, emit_tool_execution_end, emit_tool_result_message};
use super::prepare::prepare_tool_call;
use super::{ExecutedToolBatch, FinalizedToolCall, Preparation, should_terminate_tool_batch};

pub(super) async fn execute_tool_calls_sequential(
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

pub(super) async fn execute_tool_calls_parallel(
    current_context: &AgentContext,
    assistant_message: &AssistantMessage,
    tool_calls: &[ToolCall],
    config: &AgentLoopConfig,
    signal: Option<CancellationToken>,
    emit: &AgentEventCallback,
) -> ExecutedToolBatch {
    enum Entry {
        Immediate(Box<FinalizedToolCall>),
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
                entries.push(Entry::Immediate(Box::new(finalized)));
            }
            Preparation::Prepared(prepared) => {
                let emit = emit.clone();
                let signal = signal.clone();
                let current_context = current_context.clone();
                let assistant_message = assistant_message.clone();
                let config_hooks = config.after_tool_call.clone();
                let prompt_encoding = config.prompt_encoding.clone();
                entries.push(Entry::Deferred(Box::pin(async move {
                    let executed = execute_prepared_tool_call(prepared.as_ref(), signal.clone(), &emit).await;
                    finalize_executed_tool_call_with_hook(
                        &current_context,
                        &assistant_message,
                        prepared.as_ref(),
                        executed,
                        config_hooks,
                        prompt_encoding,
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

    let finalized_calls = futures::future::join_all(entries.into_iter().map(|entry| {
        let emit = emit.clone();
        async move {
            match entry {
                Entry::Immediate(f) => *f,
                Entry::Deferred(fut) => {
                    let f = fut.await;
                    emit_tool_execution_end(&f, &emit).await;
                    f
                }
            }
        }
    }))
    .await;

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
