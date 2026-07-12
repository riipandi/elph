use std::collections::HashMap;
use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::Mutex;

use elph_agent::AgentEvent;
use elph_ai::AssistantMessageEvent;
use indicatif::ProgressBar;

use crate::cli::print_tool_call;
use crate::env;
use crate::session::{TurnWriteContext, is_ask_tool};

use super::tools::{summarize_tool_args, summarize_tool_result};

pub(super) fn emit_checkpoint_warning(message: impl Into<String>) {
    eprintln!("{}", message.into());
}

pub(super) fn create_event_subscriber(
    stream: bool,
    verbose: bool,
    generating: ProgressBar,
    saw_any_delta: Arc<AtomicBool>,
    stream_ends_with_newline: Arc<AtomicBool>,
) -> elph_agent::AgentListener {
    let verbose_clone = verbose;
    let stream_clone = stream;
    Arc::new(move |event, _token| {
        let generating = generating.clone();
        let saw_any_delta = saw_any_delta.clone();
        let stream_ends_with_newline = stream_ends_with_newline.clone();
        let verbose = verbose_clone;
        let stream = stream_clone;
        Box::pin(async move {
            match event {
                AgentEvent::MessageUpdate {
                    assistant_message_event,
                    ..
                } => match &*assistant_message_event {
                    AssistantMessageEvent::TextDelta { delta, .. } => {
                        if !saw_any_delta.swap(true, Ordering::SeqCst) {
                            generating.finish_and_clear();
                        }
                        if stream {
                            stream_ends_with_newline.store(delta.ends_with('\n'), Ordering::SeqCst);
                            print!("{delta}");
                            let _ = std::io::stdout().flush();
                        }
                    }
                    AssistantMessageEvent::ThinkingDelta { delta, .. } => {
                        if !saw_any_delta.swap(true, Ordering::SeqCst) {
                            generating.finish_and_clear();
                        }
                        if verbose {
                            eprint!("\x1b[2m{delta}\x1b[0m");
                            let _ = std::io::stderr().flush();
                        }
                    }
                    _ => {}
                },
                AgentEvent::ToolExecutionStart { tool_name, .. } => {
                    if !saw_any_delta.load(Ordering::SeqCst) {
                        generating.finish_and_clear();
                    }
                    env::debug_log(format!("tool start: {tool_name}"));
                    print_tool_call(&tool_name, verbose);
                }
                AgentEvent::ToolExecutionEnd {
                    tool_name, is_error, ..
                } => {
                    env::debug_log(format!("tool end: {tool_name} error={is_error}"));
                    if verbose {
                        let icon = if is_error {
                            "\x1b[31m✗\x1b[0m"
                        } else {
                            "\x1b[32m✓\x1b[0m"
                        };
                        eprintln!("  {icon} {tool_name}");
                    }
                }
                AgentEvent::AgentEnd { .. } if !saw_any_delta.load(Ordering::SeqCst) => {
                    generating.finish_and_clear();
                }
                _ => {}
            }
        })
    })
}

pub(super) fn create_checkpoint_write_subscriber(write_ctx: TurnWriteContext) -> elph_agent::AgentListener {
    let tool_args = Arc::new(Mutex::new(HashMap::<String, String>::new()));
    Arc::new(move |event, _token| {
        let write_ctx = write_ctx.clone();
        let tool_args = tool_args.clone();
        Box::pin(async move {
            match event {
                AgentEvent::MessageUpdate {
                    assistant_message_event,
                    ..
                } => {
                    if let AssistantMessageEvent::TextDelta { delta, .. } = &*assistant_message_event
                        && let Err(err) = write_ctx.record_assistant_delta(delta).await
                    {
                        tracing::warn!(error = %err, "failed to persist assistant draft");
                        emit_checkpoint_warning(format!("Warning: checkpoint draft write failed: {err:#}"));
                    }
                }
                AgentEvent::ToolExecutionStart {
                    tool_call_id,
                    tool_name,
                    args,
                    ..
                } => {
                    let args_summary = summarize_tool_args(&tool_name, &args);
                    tool_args
                        .lock()
                        .await
                        .insert(tool_call_id.clone(), args_summary.clone());
                    if is_ask_tool(&tool_name)
                        && let Err(err) = write_ctx
                            .record_interrupt(&tool_call_id, &tool_name, &args_summary)
                            .await
                    {
                        tracing::warn!(error = %err, tool = %tool_name, "failed to persist interrupt");
                        emit_checkpoint_warning(format!(
                            "Warning: checkpoint interrupt write failed ({tool_name}): {err:#}"
                        ));
                    }
                }
                AgentEvent::ToolExecutionUpdate {
                    tool_call_id,
                    tool_name,
                    partial_result,
                    ..
                } => {
                    let args_summary = tool_args.lock().await.get(&tool_call_id).cloned().unwrap_or_default();
                    let output = summarize_tool_result(&partial_result);
                    if let Err(err) = write_ctx
                        .record_tool_partial(&tool_call_id, &tool_name, &args_summary, &output)
                        .await
                    {
                        tracing::warn!(error = %err, tool = %tool_name, "failed to persist tool partial");
                        emit_checkpoint_warning(format!(
                            "Warning: checkpoint tool partial write failed ({tool_name}): {err:#}"
                        ));
                    }
                }
                AgentEvent::ToolExecutionEnd {
                    tool_call_id,
                    tool_name,
                    is_error,
                    result,
                    ..
                } => {
                    let args_summary = tool_args.lock().await.remove(&tool_call_id).unwrap_or_default();
                    let output = summarize_tool_result(&result);
                    if is_ask_tool(&tool_name)
                        && let Err(err) = write_ctx
                            .record_resume(&tool_call_id, &tool_name, &output, is_error)
                            .await
                    {
                        tracing::warn!(error = %err, tool = %tool_name, "failed to persist resume");
                        emit_checkpoint_warning(format!(
                            "Warning: checkpoint resume write failed ({tool_name}): {err:#}"
                        ));
                    }
                    if let Err(err) = write_ctx
                        .record_tool_result(&tool_call_id, &tool_name, &args_summary, is_error, &output)
                        .await
                    {
                        tracing::warn!(error = %err, tool = %tool_name, "failed to persist tool write");
                        emit_checkpoint_warning(format!(
                            "Warning: checkpoint tool write failed ({tool_name}): {err:#}"
                        ));
                    }
                }
                _ => {}
            }
        })
    })
}
