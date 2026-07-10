//! Internal agent loop turn iteration.

use elph_ai::StopReason;
use tokio_util::sync::CancellationToken;

use crate::types::{
    AgentContext, AgentEvent, AgentLoopConfig, AgentMessage, assistant_message_to_agent, extract_tool_calls,
    tool_result_to_agent,
};

use super::AgentEventCallback;
use super::stream::stream_assistant_response;
use super::tools::{ExecutedToolBatch, execute_tool_calls, fail_tool_calls_from_truncated_message};

pub(super) async fn run_loop(
    current_context: &mut AgentContext,
    new_messages: &mut Vec<AgentMessage>,
    config: &mut AgentLoopConfig,
    signal: Option<CancellationToken>,
    emit: &AgentEventCallback,
) -> Result<(), String> {
    let mut first_turn = true;
    let mut pending_messages = if let Some(get_steering) = &config.get_steering_messages {
        get_steering().await
    } else {
        Vec::new()
    };

    loop {
        let mut has_more_tool_calls = true;

        while has_more_tool_calls || !pending_messages.is_empty() {
            if !first_turn {
                emit(AgentEvent::TurnStart).await;
            } else {
                first_turn = false;
            }

            if !pending_messages.is_empty() {
                for message in pending_messages.drain(..) {
                    emit(AgentEvent::MessageStart {
                        message: message.clone(),
                    })
                    .await;
                    emit(AgentEvent::MessageEnd {
                        message: message.clone(),
                    })
                    .await;
                    current_context.messages.push(message.clone());
                    new_messages.push(message);
                }
            }

            let message = stream_assistant_response(current_context, config, signal.clone(), emit).await?;
            new_messages.push(assistant_message_to_agent(message.clone()));

            if matches!(message.stop_reason, StopReason::Error | StopReason::Aborted) {
                emit(AgentEvent::TurnEnd {
                    message: assistant_message_to_agent(message),
                    tool_results: Vec::new(),
                })
                .await;
                emit(AgentEvent::AgentEnd {
                    messages: new_messages.clone(),
                })
                .await;
                return Ok(());
            }

            let tool_calls: Vec<_> = extract_tool_calls(&message).into_iter().cloned().collect();
            let mut tool_results = Vec::new();
            has_more_tool_calls = false;

            if !tool_calls.is_empty() {
                let batch: ExecutedToolBatch = if message.stop_reason == StopReason::Length {
                    fail_tool_calls_from_truncated_message(&tool_calls, emit).await
                } else {
                    execute_tool_calls(current_context, &message, &tool_calls, config, signal.clone(), emit).await
                };
                tool_results = batch.messages.clone();
                has_more_tool_calls = !batch.terminate;

                for result in &batch.messages {
                    let agent_msg = tool_result_to_agent(result.clone());
                    current_context.messages.push(agent_msg.clone());
                    new_messages.push(agent_msg);
                }
            }

            emit(AgentEvent::TurnEnd {
                message: assistant_message_to_agent(message.clone()),
                tool_results: tool_results.clone(),
            })
            .await;

            if let Some(prepare) = &config.prepare_next_turn {
                let snapshot = prepare(crate::types::PrepareNextTurnContext {
                    message: message.clone(),
                    tool_results: tool_results.clone(),
                    context: current_context.clone(),
                    new_messages: new_messages.clone(),
                })
                .await;
                if let Some(update) = snapshot {
                    if let Some(ctx) = update.context {
                        *current_context = ctx;
                    }
                    if let Some(model) = update.model {
                        config.model = model;
                    }
                    if let Some(level) = update.thinking_level {
                        config.stream_options.reasoning = level.to_stream_reasoning();
                    }
                }
            }

            if let Some(should_stop) = &config.should_stop_after_turn
                && should_stop(crate::types::ShouldStopAfterTurnContext {
                    message: message.clone(),
                    tool_results: tool_results.clone(),
                    context: current_context.clone(),
                    new_messages: new_messages.clone(),
                })
                .await
            {
                emit(AgentEvent::AgentEnd {
                    messages: new_messages.clone(),
                })
                .await;
                return Ok(());
            }

            pending_messages = if let Some(get_steering) = &config.get_steering_messages {
                get_steering().await
            } else {
                Vec::new()
            };
        }

        let follow_up = if let Some(get_follow_up) = &config.get_follow_up_messages {
            get_follow_up().await
        } else {
            Vec::new()
        };

        if !follow_up.is_empty() {
            pending_messages = follow_up;
            continue;
        }

        break;
    }

    emit(AgentEvent::AgentEnd {
        messages: new_messages.clone(),
    })
    .await;
    Ok(())
}
