//! Memory hooks: automatic recall, auto-correction, work/discovery capture, and task lifecycle.
//!
//! All hooks share a session-scoped [`MemoryRuntime`] (no dual store / thread-local task id).

use std::sync::Arc;

use anyhow::Result;
use serde_json::Value;

use elph_agent::{
    AgentEvent, AgentHarness, AgentHarnessEvent, BeforeAgentStartEvent, BeforeAgentStartResult, SessionDirStorage,
    ToolResultEvent,
};
use elph_ai::{Message, Usage};
use floppy::{ReportCorrectionInput, ReportUserInput, UserInputSource};

use super::runtime::{build_self_report, MemoryRuntime};

/// Keywords that suggest a user message is a correction (not a normal continuation).
fn is_user_correction(text: &str) -> Option<&'static str> {
    let lower = text.trim().to_lowercase();
    let patterns = &[
        ("jangan", "id:jangan"),
        ("seharusnya", "id:seharusnya"),
        ("sebaiknya", "id:sebaiknya"),
        ("harusnya", "id:harusnya"),
        ("bukan begitu", "id:bukan_begitu"),
        ("salah", "id:salah"),
        ("wrong approach", "en:wrong_approach"),
        ("that's not", "en:thats_not"),
        ("that is not", "en:that_is_not"),
        ("instead, use", "en:instead_use"),
        ("actually, use", "en:actually_use"),
        ("don't", "en:dont"),
        ("do not", "en:do_not"),
        ("no,", "en:no_comma"),
    ];
    for (pat, label) in patterns {
        if lower.contains(pat) {
            return Some(label);
        }
    }
    None
}

fn extract_usage_from_agent_message(msg: &elph_agent::AgentMessage) -> Option<Usage> {
    let llm = msg.as_llm()?;
    match llm {
        elph_ai::Message::Assistant(assistant) => {
            if assistant.usage.total_tokens > 0 {
                Some(assistant.usage.clone())
            } else {
                None
            }
        }
        _ => None,
    }
}

fn format_tool_error_lesson(tool_name: &str, args: &Value) -> String {
    let args_preview = serde_json::to_string(args).unwrap_or_default();
    let args_short = if args_preview.chars().count() > 300 {
        let truncated: String = args_preview.chars().take(300).collect();
        format!("{truncated}...")
    } else {
        args_preview
    };
    format!("Tool `{tool_name}` failed with args: {args_short}")
}

fn assistant_message_has_error(msg: &elph_agent::AgentMessage) -> bool {
    let llm = match msg.as_llm() {
        Some(m) => m,
        None => return false,
    };
    match llm {
        elph_ai::Message::Assistant(assistant) => {
            assistant.error_message.is_some() || matches!(assistant.stop_reason, elph_ai::StopReason::Error)
        }
        _ => false,
    }
}

fn count_tool_result_errors(tool_results: &[Message]) -> u32 {
    tool_results
        .iter()
        .filter(|m| matches!(m, Message::ToolResult { is_error: true, .. }))
        .count() as u32
}

/// Register automatic memory hooks on the harness using the shared runtime.
pub async fn register_automatic_memory_hooks(
    harness: &AgentHarness<SessionDirStorage>,
    runtime: Arc<MemoryRuntime>,
) -> Result<()> {
    if !runtime.is_enabled() {
        log::info!("automatic memory hooks: disabled via memory.enabled=false");
        return Ok(());
    }

    // -------------------------------------------------------------------
    // Hook A: before_agent_start — multi-source recall + user correction
    // -------------------------------------------------------------------
    harness
        .on_before_agent_start({
            let runtime = Arc::clone(&runtime);
            move |event: &BeforeAgentStartEvent| {
                let runtime = Arc::clone(&runtime);
                let prompt = event.prompt.clone();
                let system_prompt = event.system_prompt.clone();
                Box::pin(async move {
                    let prompt_len = prompt.trim().chars().count();
                    runtime.begin_turn(&prompt);
                    log::debug!("memory.recall.start prompt_len={prompt_len}");

                    // --- Step 1: detect user corrections ---
                    if is_user_correction(&prompt).is_some() {
                        runtime.bump_user_correction();
                        let lesson = format!("User correction: {}", prompt.trim());
                        let _ = runtime
                            .report_user_input(ReportUserInput {
                                lesson,
                                source: UserInputSource::UserCorrection,
                            })
                            .await;
                    }

                    // --- Step 2: skip short queries ---
                    let min_len = runtime.options().min_query_length;
                    if prompt_len < min_len {
                        log::debug!(
                            "memory.recall.start skipped_reason=short_prompt prompt_len={prompt_len} min={min_len}"
                        );
                        return None;
                    }

                    // --- Step 3–5: multi-source rank + pack ---
                    let recall = match runtime.build_turn_context(&prompt).await {
                        Ok(Some(r)) => r,
                        Ok(None) => {
                            log::debug!("memory.recall.start skipped_reason=no_relevant_hits");
                            return None;
                        }
                        Err(err) => {
                            log::debug!("memory.recall.start skipped_reason=build_failed err={err:#}");
                            return None;
                        }
                    };

                    runtime.set_injected_ids(recall.injected_ids);

                    let new_prompt = if system_prompt.is_empty() {
                        recall.context
                    } else {
                        format!("{system_prompt}\n\n{}", recall.context)
                    };

                    Some(BeforeAgentStartResult {
                        system_prompt: Some(new_prompt),
                        messages: None,
                    })
                })
            }
        })
        .await;

    // -------------------------------------------------------------------
    // Hook B: on_tool_result — errors, mutations, exploration
    // -------------------------------------------------------------------
    harness
        .on_tool_result({
            let runtime = Arc::clone(&runtime);
            move |event: &ToolResultEvent| {
                let runtime = Arc::clone(&runtime);
                let tool_name = event.tool_name.clone();
                let args = event.input.clone();
                let is_error = event.is_error;
                Box::pin(async move {
                    if is_error {
                        let lesson = format_tool_error_lesson(&tool_name, &args);
                        let what_failed = format!("Tool execution error: {tool_name}");
                        let _ = runtime
                            .report_correction(ReportCorrectionInput {
                                lesson,
                                what_failed,
                                what_worked: "unknown".into(),
                                tokens_wasted: None,
                                tools_wasted: None,
                            })
                            .await;
                    } else {
                        runtime.record_successful_mutation(&tool_name, &args);
                        runtime.record_successful_exploration(&tool_name, &args);
                    }
                    None
                })
            }
        })
        .await;

    // -------------------------------------------------------------------
    // Hook C: TurnEnd — work + discovery flush, end_task
    // -------------------------------------------------------------------
    harness
        .subscribe({
            let runtime = Arc::clone(&runtime);
            move |event: AgentHarnessEvent, _signal| {
                let runtime = Arc::clone(&runtime);
                Box::pin(async move {
                    let agent_event = match event {
                        AgentHarnessEvent::Agent(e) => e,
                        _ => return,
                    };
                    let (message, tool_results) = match agent_event {
                        AgentEvent::TurnEnd { message, tool_results } => (message, tool_results),
                        _ => return,
                    };

                    let has_error = assistant_message_has_error(&message);
                    let completed = !has_error;
                    let tokens_used = extract_usage_from_agent_message(&message)
                        .map(|u| u.output as u32)
                        .unwrap_or(0);
                    let tool_calls = tool_results.len() as u32;
                    let mut errors = count_tool_result_errors(&tool_results);
                    if errors == 0 && has_error {
                        errors = 1;
                    }

                    let scratch = runtime.take_turn_scratch();

                    // Flush work/change then discoveries while task may still be active.
                    runtime.flush_turn_work(&scratch, completed).await;
                    runtime.flush_turn_discoveries(&scratch).await;

                    if runtime.active_task_id().is_none() {
                        return;
                    }

                    let input = floppy::TaskEndInput {
                        tokens_used,
                        tool_calls,
                        errors,
                        user_corrections: scratch.user_corrections,
                        completed,
                        self_report: build_self_report(&scratch, completed),
                    };

                    let _ = runtime.end_active_task(input).await;
                })
            }
        })
        .await;

    Ok(())
}

/// Session-start memory note (turn-only recall carries the real payload).
pub async fn build_memories_context(runtime: &MemoryRuntime) -> Result<String> {
    runtime.build_bootstrap_context().await
}

/// Best-effort end-of-session maintenance (embed pending + decay).
pub async fn session_end_maintenance(runtime: &MemoryRuntime) {
    runtime.session_end_maintenance().await;
}
