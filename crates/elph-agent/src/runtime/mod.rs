//! Agent runtime — execution engine + async bridge.

pub mod env;
pub mod event_stream;
mod exec;
pub mod local_env;
pub mod loop_config;
pub mod proxy;
mod run_loop;
mod stream;

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use anyhow::Result;
use tokio_util::sync::CancellationToken;

use self::event_stream::AgentEventStream;
use crate::types::{AgentContext, AgentEvent, AgentLoopConfig, AgentMessage};

pub use exec::fail_tool_calls_from_truncated_message;

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

    log::debug!("agent loop start messages={}", current_context.messages.len());
    match run_loop::run_loop(&mut current_context, &mut new_messages, &mut config, signal, &emit).await {
        Ok(()) => {
            log::debug!("agent loop ok new_messages={}", new_messages.len());
            Ok(new_messages)
        }
        Err(error) => {
            log::warn!("agent loop failed: {error}");
            Err(error)
        }
    }
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
    log::debug!("agent loop continue start messages={}", current_context.messages.len());
    match run_loop::run_loop(&mut current_context, &mut new_messages, &mut config, signal, &emit).await {
        Ok(()) => {
            log::debug!("agent loop continue ok new_messages={}", new_messages.len());
            Ok(new_messages)
        }
        Err(error) => {
            log::warn!("agent loop continue failed: {error}");
            Err(error)
        }
    }
}

fn run_future<F, T>(future: F) -> Result<T>
where
    F: Future<Output = T>,
{
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        return Ok(tokio::task::block_in_place(|| handle.block_on(future)));
    }

    Ok(tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(future))
}

/// Runs an async future, panicking if the runtime cannot be created.
pub fn block_on<F, T>(future: F) -> T
where
    F: Future<Output = T>,
{
    run_future(future).expect("failed to run async task")
}

/// Runs an async future, returning errors from runtime construction.
pub fn try_block_on<F, T>(future: F) -> Result<T>
where
    F: Future<Output = T>,
{
    run_future(future)
}

/// Runs a `Send` future on a **dedicated OS thread** with its own current-thread
/// runtime, optionally bounded by `timeout`.
///
/// Use this from TUI / sync input handlers that may already sit inside
/// `block_in_place` + `Handle::block_on` (the iocraft render loop). Nested
/// `try_block_on` on the same runtime can panic or deadlock while the agent is
/// streaming; a detached thread avoids both.
pub fn try_block_on_detached<F, T>(future: F, timeout: std::time::Duration) -> Result<T>
where
    F: Future<Output = T> + Send + 'static,
    T: Send + 'static,
{
    let (tx, rx) = std::sync::mpsc::channel();
    let worker_timeout = timeout;
    std::thread::Builder::new()
        .name("elph-detached-async".into())
        .spawn(move || {
            let result = (|| {
                let rt = tokio::runtime::Builder::new_current_thread().enable_all().build()?;
                if worker_timeout.is_zero() {
                    Ok(rt.block_on(future))
                } else {
                    match rt.block_on(async { tokio::time::timeout(worker_timeout, future).await }) {
                        Ok(value) => Ok(value),
                        Err(_elapsed) => {
                            Err(anyhow::anyhow!("async work timed out after {}ms", worker_timeout.as_millis()))
                        }
                    }
                }
            })();
            let _ = tx.send(result);
        })
        .map_err(|err| anyhow::anyhow!("failed to spawn detached async worker: {err}"))?;

    let join_budget = if timeout.is_zero() {
        std::time::Duration::from_secs(30)
    } else {
        timeout.saturating_add(std::time::Duration::from_millis(250))
    };
    match rx.recv_timeout(join_budget) {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(err)) => Err(err),
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
            Err(anyhow::anyhow!("async work timed out after {}ms", timeout.as_millis()))
        }
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
            Err(anyhow::anyhow!("detached async worker exited without a result"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn try_block_on_works_outside_runtime() {
        let value = try_block_on(async { 42 }).expect("outside runtime");
        assert_eq!(value, 42);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn try_block_on_works_inside_runtime() {
        let value = try_block_on(async { 42 }).expect("inside runtime");
        assert_eq!(value, 42);
    }

    #[test]
    fn try_block_on_detached_runs_off_caller_thread() {
        let value = try_block_on_detached(async { 7u32 }, std::time::Duration::from_secs(1)).expect("detached");
        assert_eq!(value, 7);
    }

    #[test]
    fn try_block_on_detached_times_out() {
        let err = try_block_on_detached(
            async {
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                1u32
            },
            std::time::Duration::from_millis(50),
        )
        .expect_err("should time out");
        assert!(err.to_string().contains("timed out"), "{err}");
    }
}
