//! Agent loop — elph-agent module.

mod run_loop;
mod stream;
mod tools;

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use tokio_util::sync::CancellationToken;

use crate::event_stream::AgentEventStream;
use crate::types::{AgentContext, AgentEvent, AgentLoopConfig, AgentMessage};

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
    let emit = stream::event_callback(stream_clone);
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
    let emit = stream::event_callback(stream_clone);
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

#[cfg_attr(feature = "tracing", fastrace::trace(name = "elph.agent.loop"))]
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

    run_loop::run_loop(&mut current_context, &mut new_messages, &mut config, signal, &emit).await?;
    Ok(new_messages)
}

#[cfg_attr(feature = "tracing", fastrace::trace(name = "elph.agent.loop_continue"))]
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
    run_loop::run_loop(&mut current_context, &mut new_messages, &mut config, signal, &emit).await?;
    Ok(new_messages)
}
