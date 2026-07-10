//! Assistant response streaming and event callback helpers.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use elph_ai::utils::event_stream::AssistantMessageEventStream;
use elph_ai::{AssistantMessage, AssistantMessageEvent, Context, SimpleStreamOptions};
use tokio_util::sync::CancellationToken;

use crate::event_stream::AgentEventStream;
use crate::types::{AgentContext, AgentEvent, AgentLoopConfig, assistant_message_to_agent};

use super::AgentEventCallback;

pub(super) async fn stream_assistant_response(
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

pub(super) fn event_callback(stream: AgentEventStream) -> AgentEventCallback {
    Arc::new(move |event| {
        let stream = stream.clone();
        Box::pin(async move {
            stream.push(event);
        }) as Pin<Box<dyn Future<Output = ()> + Send>>
    })
}
