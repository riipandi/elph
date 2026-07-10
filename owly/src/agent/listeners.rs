use std::collections::HashMap;
use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::{Mutex, mpsc};

use elph_agent::AgentEvent;
use elph_ai::AssistantMessageEvent;
use indicatif::ProgressBar;

use crate::cli::print_tool_call;
use crate::env;
use crate::session::{TurnWriteContext, is_ask_tool};
use crate::ui_events::AgentUiEvent;

use super::tools::{summarize_tool_args, summarize_tool_result};

pub(super) fn emit_ui(ui: &Option<mpsc::UnboundedSender<AgentUiEvent>>, event: AgentUiEvent) {
    if let Some(tx) = ui {
        let _ = tx.send(event);
    }
}

pub(super) fn emit_checkpoint_warning(
    ui_events: &Option<mpsc::UnboundedSender<AgentUiEvent>>,
    quiet: bool,
    message: impl Into<String>,
) {
    let message = message.into();
    emit_ui(ui_events, AgentUiEvent::Status(message.clone()));
    if ui_events.is_none() && !quiet {
        eprintln!("{message}");
    }
}

pub(super) fn create_event_subscriber(
    stream: bool,
    verbose: bool,
    generating: ProgressBar,
    saw_any_delta: Arc<AtomicBool>,
) -> elph_agent::AgentListener {
    let verbose_clone = verbose;
    let stream_clone = stream;
    Arc::new(move |event, _token| {
        let generating = generating.clone();
        let saw_any_delta = saw_any_delta.clone();
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
                        if stream || verbose {
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

pub(super) fn create_checkpoint_write_subscriber(
    write_ctx: TurnWriteContext,
    ui_events: Option<mpsc::UnboundedSender<AgentUiEvent>>,
    quiet: bool,
) -> elph_agent::AgentListener {
    let tool_args = Arc::new(Mutex::new(HashMap::<String, String>::new()));
    Arc::new(move |event, _token| {
        let write_ctx = write_ctx.clone();
        let tool_args = tool_args.clone();
        let ui_events = ui_events.clone();
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
                        emit_checkpoint_warning(
                            &ui_events,
                            quiet,
                            format!("Warning: checkpoint draft write failed: {err:#}"),
                        );
                    }
                }
                AgentEvent::ToolExecutionStart {
                    tool_call_id,
                    tool_name,
                    args,
                    ..
                } => {
                    let args_summary = summarize_tool_args(&args);
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
                        emit_checkpoint_warning(
                            &ui_events,
                            quiet,
                            format!("Warning: checkpoint interrupt write failed ({tool_name}): {err:#}"),
                        );
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
                        emit_checkpoint_warning(
                            &ui_events,
                            quiet,
                            format!("Warning: checkpoint tool partial write failed ({tool_name}): {err:#}"),
                        );
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
                        emit_checkpoint_warning(
                            &ui_events,
                            quiet,
                            format!("Warning: checkpoint resume write failed ({tool_name}): {err:#}"),
                        );
                    }
                    if let Err(err) = write_ctx
                        .record_tool_result(&tool_call_id, &tool_name, &args_summary, is_error, &output)
                        .await
                    {
                        tracing::warn!(error = %err, tool = %tool_name, "failed to persist tool write");
                        emit_checkpoint_warning(
                            &ui_events,
                            quiet,
                            format!("Warning: checkpoint tool write failed ({tool_name}): {err:#}"),
                        );
                    }
                }
                _ => {}
            }
        })
    })
}

pub(super) fn create_tui_event_subscriber(
    ui_events: mpsc::UnboundedSender<AgentUiEvent>,
    stream_text: bool,
    show_thinking: bool,
) -> elph_agent::AgentListener {
    Arc::new(move |event, _token| {
        let ui_events = ui_events.clone();
        Box::pin(async move {
            let mapped = match event {
                AgentEvent::MessageUpdate {
                    assistant_message_event,
                    ..
                } => match &*assistant_message_event {
                    AssistantMessageEvent::TextDelta { delta, .. } if stream_text => {
                        Some(AgentUiEvent::TextDelta(delta.clone()))
                    }
                    AssistantMessageEvent::ThinkingDelta { delta, .. } if show_thinking => {
                        Some(AgentUiEvent::ThinkingDelta(delta.clone()))
                    }
                    _ => None,
                },
                AgentEvent::ToolExecutionStart {
                    tool_call_id,
                    tool_name,
                    args,
                    ..
                } => Some(AgentUiEvent::ToolStart {
                    id: tool_call_id.clone(),
                    name: tool_name.clone(),
                    args_summary: summarize_tool_args(&args),
                }),
                AgentEvent::ToolExecutionUpdate {
                    tool_call_id,
                    partial_result,
                    ..
                } => {
                    let output = summarize_tool_result(&partial_result);
                    if output.is_empty() {
                        None
                    } else {
                        Some(AgentUiEvent::ToolUpdate {
                            id: tool_call_id.clone(),
                            output,
                        })
                    }
                }
                AgentEvent::ToolExecutionEnd {
                    tool_call_id,
                    is_error,
                    result,
                    ..
                } => Some(AgentUiEvent::ToolEnd {
                    id: tool_call_id.clone(),
                    is_error,
                    output: summarize_tool_result(&result),
                }),
                _ => None,
            };
            if let Some(mapped) = mapped {
                let _ = ui_events.send(mapped);
            }
        })
    })
}
