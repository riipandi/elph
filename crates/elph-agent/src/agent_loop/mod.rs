//! Agent loop — ported from pi-agent `agent-loop.ts`.

mod tools;

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use elph_ai::{AssistantMessage, AssistantMessageEvent, Context, SimpleStreamOptions, StopReason};
use tokio_util::sync::CancellationToken;

use crate::event_stream::AgentEventStream;
use crate::types::{
    AgentContext, AgentEvent, AgentLoopConfig, AgentMessage, assistant_message_to_agent, extract_tool_calls,
    tool_result_to_agent,
};
use elph_ai::utils::event_stream::AssistantMessageEventStream;
use tools::{ExecutedToolBatch, execute_tool_calls};

pub use tools::fail_tool_calls_from_truncated_message;

pub type AgentEventCallback = Arc<dyn Fn(AgentEvent) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>;

pub fn agent_loop(
    prompts: Vec<AgentMessage>,
    context: AgentContext,
    config: AgentLoopConfig,
    signal: Option<CancellationToken>,
    stream_fn: Option<crate::types::StreamFn>,
) -> AgentEventStream {
    let stream = AgentEventStream::new();
    let stream_clone = stream.clone();
    let emit = event_callback(stream_clone);
    let mut config = config;
    if stream_fn.is_some() {
        config.stream_fn = stream_fn;
    }

    let stream_for_task = stream.clone();
    tokio::spawn(async move {
        let result: Vec<AgentMessage> = run_agent_loop(prompts, context, config, emit, signal)
            .await
            .unwrap_or_default();
        stream_for_task.end(result);
    });

    stream
}

pub fn agent_loop_continue(
    context: AgentContext,
    config: AgentLoopConfig,
    signal: Option<CancellationToken>,
    stream_fn: Option<crate::types::StreamFn>,
) -> AgentEventStream {
    if context.messages.is_empty() {
        panic!("Cannot continue: no messages in context");
    }
    if context.messages.last().is_some_and(|m| m.role() == "assistant") {
        panic!("Cannot continue from message role: assistant");
    }

    let stream = AgentEventStream::new();
    let stream_clone = stream.clone();
    let emit = event_callback(stream_clone);
    let mut config = config;
    if stream_fn.is_some() {
        config.stream_fn = stream_fn;
    }

    let stream_for_task = stream.clone();
    tokio::spawn(async move {
        let result: Vec<AgentMessage> = run_agent_loop_continue(context, config, emit, signal)
            .await
            .unwrap_or_default();
        stream_for_task.end(result);
    });

    stream
}

pub async fn run_agent_loop(
    prompts: Vec<AgentMessage>,
    context: AgentContext,
    mut config: AgentLoopConfig,
    emit: AgentEventCallback,
    signal: Option<CancellationToken>,
) -> Result<Vec<AgentMessage>, String> {
    let mut new_messages = prompts.clone();
    let mut current_context = AgentContext {
        system_prompt: context.system_prompt,
        messages: {
            let mut msgs = context.messages;
            msgs.extend(prompts.clone());
            msgs
        },
        tools: context.tools,
    };

    emit(AgentEvent::AgentStart).await;
    emit(AgentEvent::TurnStart).await;
    for prompt in &prompts {
        emit(AgentEvent::MessageStart {
            message: prompt.clone(),
        })
        .await;
        emit(AgentEvent::MessageEnd {
            message: prompt.clone(),
        })
        .await;
    }

    run_loop(&mut current_context, &mut new_messages, &mut config, signal, &emit).await?;
    Ok(new_messages)
}

pub async fn run_agent_loop_continue(
    context: AgentContext,
    config: AgentLoopConfig,
    emit: AgentEventCallback,
    signal: Option<CancellationToken>,
) -> Result<Vec<AgentMessage>, String> {
    if context.messages.is_empty() {
        panic!("Cannot continue: no messages in context");
    }
    if context.messages.last().is_some_and(|m| m.role() == "assistant") {
        panic!("Cannot continue from message role: assistant");
    }

    let mut new_messages = Vec::new();
    let mut current_context = context;

    emit(AgentEvent::AgentStart).await;
    emit(AgentEvent::TurnStart).await;

    let mut config = config;
    run_loop(&mut current_context, &mut new_messages, &mut config, signal, &emit).await?;
    Ok(new_messages)
}

async fn run_loop(
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

async fn stream_assistant_response(
    context: &mut AgentContext,
    config: &AgentLoopConfig,
    signal: Option<CancellationToken>,
    emit: &AgentEventCallback,
) -> Result<AssistantMessage, String> {
    let messages = if let Some(transform) = &config.transform_context {
        transform(context.messages.clone(), signal.clone()).await?
    } else {
        context.messages.clone()
    };

    let llm_messages = (config.convert_to_llm)(messages).await;
    let llm_tools: Vec<elph_ai::Tool> = context.tools.iter().map(|t| t.tool.clone()).collect();

    let llm_context = Context {
        system_prompt: Some(context.system_prompt.clone()),
        messages: llm_messages,
        tools: if llm_tools.is_empty() { None } else { Some(llm_tools) },
    };

    let mut stream_options = config.stream_options.clone();
    if let Some(token) = signal {
        stream_options.base.signal = Some(token);
    }

    if let Some(get_key) = &config.get_api_key
        && let Some(key) = get_key(&config.model.provider).await
    {
        stream_options.base.api_key = Some(key);
    }

    let stream = if let Some(stream_fn) = &config.stream_fn {
        stream_fn(&config.model, &llm_context, Some(stream_options))
    } else {
        default_stream_fn(&config.model, &llm_context, Some(stream_options))
    };

    let mut partial_message: Option<AssistantMessage> = None;
    let mut added_partial = false;

    let mut events = stream.clone().into_stream();
    while let Some(event) = events.next().await {
        match &event {
            AssistantMessageEvent::Start { partial } => {
                partial_message = Some(partial.clone());
                context.messages.push(assistant_message_to_agent(partial.clone()));
                added_partial = true;
                emit(AgentEvent::MessageStart {
                    message: assistant_message_to_agent(partial.clone()),
                })
                .await;
            }
            AssistantMessageEvent::TextStart { partial, .. }
            | AssistantMessageEvent::TextDelta { partial, .. }
            | AssistantMessageEvent::TextEnd { partial, .. }
            | AssistantMessageEvent::ThinkingStart { partial, .. }
            | AssistantMessageEvent::ThinkingDelta { partial, .. }
            | AssistantMessageEvent::ThinkingEnd { partial, .. }
            | AssistantMessageEvent::ToolcallStart { partial, .. }
            | AssistantMessageEvent::ToolcallDelta { partial, .. }
            | AssistantMessageEvent::ToolcallEnd { partial, .. } => {
                if partial_message.is_some() {
                    partial_message = Some(partial.clone());
                    if let Some(last) = context.messages.last_mut() {
                        *last = assistant_message_to_agent(partial.clone());
                    }
                    emit(AgentEvent::MessageUpdate {
                        message: assistant_message_to_agent(partial.clone()),
                        assistant_message_event: Box::new(event.clone()),
                    })
                    .await;
                }
            }
            AssistantMessageEvent::Done { .. } | AssistantMessageEvent::Error { .. } => {
                let final_message = stream.result().await;
                if added_partial {
                    if let Some(last) = context.messages.last_mut() {
                        *last = assistant_message_to_agent(final_message.clone());
                    }
                } else {
                    context.messages.push(assistant_message_to_agent(final_message.clone()));
                }
                if !added_partial {
                    emit(AgentEvent::MessageStart {
                        message: assistant_message_to_agent(final_message.clone()),
                    })
                    .await;
                }
                emit(AgentEvent::MessageEnd {
                    message: assistant_message_to_agent(final_message.clone()),
                })
                .await;
                return Ok(final_message);
            }
        }
    }

    let final_message = stream.result().await;
    if added_partial {
        if let Some(last) = context.messages.last_mut() {
            *last = assistant_message_to_agent(final_message.clone());
        }
    } else {
        context.messages.push(assistant_message_to_agent(final_message.clone()));
        emit(AgentEvent::MessageStart {
            message: assistant_message_to_agent(final_message.clone()),
        })
        .await;
    }
    emit(AgentEvent::MessageEnd {
        message: assistant_message_to_agent(final_message.clone()),
    })
    .await;
    Ok(final_message)
}

fn default_stream_fn(
    model: &elph_ai::Model,
    context: &Context,
    options: Option<SimpleStreamOptions>,
) -> AssistantMessageEventStream {
    elph_ai::builtin_models(None).stream_simple(model, context, options)
}

fn event_callback(stream: AgentEventStream) -> AgentEventCallback {
    Arc::new(move |event| {
        let stream = stream.clone();
        Box::pin(async move {
            stream.push(event);
        })
    })
}
