//! Harness event wiring and UI event mapping.

use anyhow::Result;
use elph_agent::{AgentEvent, AgentHarnessEvent, AgentHarnessOwnEvent, FileSystem};
use elph_agent::{SubagentEventForwarder, SubagentInfo, ToolCallEvent, ToolCallHookResult};
use elph_ai::AssistantMessageEvent;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

use super::CodingAgentSession;

use crate::agent::events::{AgentUiEvent, PlanConfirmationRequest, QueuedPromptItem, QueuedPromptKind};

impl CodingAgentSession {
    pub(super) async fn wire_harness(&self, ui_tx: mpsc::UnboundedSender<AgentUiEvent>) -> Result<()> {
        let harness = self.harness.clone();
        let policy = self.policy.clone();
        let show_thinking = self.show_thinking;
        let cwd = self.harness().env().cwd().to_string();

        harness
            .on_tool_call({
                let ui_tx = ui_tx.clone();
                let policy = Arc::clone(&policy);
                let cwd = cwd.clone();
                move |event: &ToolCallEvent| {
                    let ui_tx = ui_tx.clone();
                    let policy = Arc::clone(&policy);
                    let cwd = cwd.clone();
                    let tool_call_id = event.tool_call_id.clone();
                    let tool_name = event.tool_name.clone();
                    let mut args_summary = serde_json::to_string(&event.input).unwrap_or_default();
                    if tool_name == "shell_exec" {
                        args_summary = elph_agent::normalize_shell_exec_args(&args_summary, &cwd);
                    }
                    Box::pin(async move {
                        let policy = policy.lock().await;
                        if !policy.needs_approval(&tool_name) {
                            return None;
                        }
                        match policy
                            .request_approval(tool_call_id, tool_name, args_summary, &ui_tx)
                            .await
                        {
                            Ok(true) => None,
                            Ok(false) => Some(ToolCallHookResult {
                                block: true,
                                reason: Some("Tool execution rejected by user".into()),
                            }),
                            Err(reason) => Some(ToolCallHookResult {
                                block: true,
                                reason: Some(reason),
                            }),
                        }
                    })
                }
            })
            .await;

        harness
            .subscribe({
                let ui_tx = ui_tx.clone();
                let cwd = cwd.clone();
                move |event, _| {
                    let ui_tx = ui_tx.clone();
                    let cwd = cwd.clone();
                    Box::pin(async move {
                        if let AgentHarnessEvent::Agent(agent_event) = event {
                            map_agent_event(&ui_tx, agent_event, show_thinking, &cwd);
                        } else if let AgentHarnessEvent::Own(AgentHarnessOwnEvent::QueueUpdate(update)) = event {
                            let items = map_queue_update(&update);
                            let _ = ui_tx.send(AgentUiEvent::QueueUpdate { items });
                        }
                    })
                }
            })
            .await;

        // Resolve active model id for subagent status display (e.g. `claude-sonnet-4-20250514`).
        let active_model_id = self.selection.read().model.id.clone();

        // Accumulator per agent for output text deltas.
        // Batching: flush after every N deltas to avoid flooding the UI channel.
        const BATCH_INTERVAL: usize = 8;

        let forwarder: SubagentEventForwarder = Arc::new({
            let ui_tx = ui_tx.clone();
            let model_id = active_model_id.clone();
            let output_buf: Arc<Mutex<HashMap<String, (String, usize)>>> = Arc::new(Mutex::new(HashMap::new()));
            move |event, info: &SubagentInfo| {
                use crate::agent::SubagentUiPhase;

                let mut buf = output_buf.lock().unwrap();
                let entry = buf.entry(info.id.clone()).or_insert_with(|| (String::new(), 0));

                match event {
                    AgentEvent::AgentStart => {
                        let _ = ui_tx.send(AgentUiEvent::SubagentStatus {
                            agent_id: info.id.clone(),
                            agent_path: info.agent_path.clone(),
                            task_name: info.task_name.clone(),
                            phase: SubagentUiPhase::Running,
                            message: String::new(),
                            model: model_id.clone(),
                        });
                    }
                    AgentEvent::AgentEnd { .. } => {
                        // Flush remaining output, then send completion marker.
                        if !entry.0.is_empty() {
                            let _ = ui_tx.send(AgentUiEvent::SubagentOutput {
                                agent_id: info.id.clone(),
                                content: std::mem::take(&mut entry.0),
                            });
                            entry.1 = 0;
                        }
                        let _ = ui_tx.send(AgentUiEvent::SubagentStatus {
                            agent_id: info.id.clone(),
                            agent_path: info.agent_path.clone(),
                            task_name: info.task_name.clone(),
                            phase: SubagentUiPhase::Done,
                            message: String::new(),
                            model: model_id.clone(),
                        });
                    }
                    // Tool activity: upsert running row with human verb.
                    AgentEvent::ToolExecutionStart { tool_name, .. } => {
                        let _ = ui_tx.send(AgentUiEvent::SubagentStatus {
                            agent_id: info.id.clone(),
                            agent_path: info.agent_path.clone(),
                            task_name: info.task_name.clone(),
                            phase: SubagentUiPhase::Running,
                            message: format!("tool:{tool_name}"),
                            model: model_id.clone(),
                        });
                    }
                    AgentEvent::ToolExecutionEnd {
                        tool_name,
                        is_error: true,
                        ..
                    } => {
                        let _ = ui_tx.send(AgentUiEvent::SubagentStatus {
                            agent_id: info.id.clone(),
                            agent_path: info.agent_path.clone(),
                            task_name: info.task_name.clone(),
                            phase: SubagentUiPhase::Error,
                            message: format!("tool:{tool_name}"),
                            model: model_id.clone(),
                        });
                    }
                    // — Output deltas: accumulate into the buffer —
                    AgentEvent::MessageUpdate {
                        ref assistant_message_event,
                        ..
                    } => {
                        let delta = match &**assistant_message_event {
                            AssistantMessageEvent::TextDelta { delta, .. } => delta.as_str(),
                            AssistantMessageEvent::ThinkingDelta { delta, .. } => delta.as_str(),
                            _ => return,
                        };
                        entry.0.push_str(delta);
                        entry.1 += 1;
                        if entry.1 >= BATCH_INTERVAL {
                            let _ = ui_tx.send(AgentUiEvent::SubagentOutput {
                                agent_id: info.id.clone(),
                                content: std::mem::take(&mut entry.0),
                            });
                            entry.1 = 0;
                        }
                    }
                    AgentEvent::ToolExecutionUpdate { ref partial_result, .. } => {
                        let output = summarize_tool_result(partial_result);
                        if !output.is_empty() {
                            entry.0.push_str(&output);
                            entry.1 += 1;
                            if entry.1 >= BATCH_INTERVAL {
                                let _ = ui_tx.send(AgentUiEvent::SubagentOutput {
                                    agent_id: info.id.clone(),
                                    content: std::mem::take(&mut entry.0),
                                });
                                entry.1 = 0;
                            }
                        }
                    }
                    // Successful tool execution: append result text.
                    AgentEvent::ToolExecutionEnd {
                        ref result,
                        is_error: false,
                        ..
                    } => {
                        let output = summarize_tool_result(result);
                        if !output.is_empty() {
                            entry.0.push_str(&output);
                            entry.1 += 1;
                            if entry.1 >= BATCH_INTERVAL {
                                let _ = ui_tx.send(AgentUiEvent::SubagentOutput {
                                    agent_id: info.id.clone(),
                                    content: std::mem::take(&mut entry.0),
                                });
                                entry.1 = 0;
                            }
                        }
                    }
                    _ => {}
                }
            }
        });
        self.harness
            .agent_control()
            .await
            .set_event_forwarder(Some(forwarder))
            .await;

        // ── Tool execution output persistence ──────────────────────────────────
        // Persist every tool call result to `tool_outputs.jsonl` in the session
        // directory so results survive session resume and can be browsed.
        let harness_for_tool_output = self.harness.clone();
        let tool_sink = harness_for_tool_output.clone();
        tool_sink
            .on_tool_result({
                move |event: &elph_agent::ToolResultEvent| {
                    let harness = harness_for_tool_output.clone();
                    let event = event.clone();
                    Box::pin(async move {
                        let meta = harness.session_metadata().await;
                        // Tool outputs: APP_DATA/sessions/<SESSION_ID>/tool_outputs.jsonl
                        let dir = crate::platform::Paths::session_artifact_dir_from_db(
                            std::path::Path::new(&meta.db_path),
                            &meta.id,
                        );
                        let _ = tokio::fs::create_dir_all(&dir).await;
                        let _ = elph_agent::session::backends::session_dir::tool_outputs::append_tool_output(
                            &dir,
                            &event.tool_call_id,
                            &event.tool_name,
                            &event.input,
                            &event
                                .content
                                .iter()
                                .filter_map(|c| match c {
                                    elph_agent::ToolResultContent::Text(t) => Some(t.text.as_str()),
                                    _ => None,
                                })
                                .collect::<Vec<_>>()
                                .join("\n"),
                            event.is_error,
                        )
                        .await;
                        None::<elph_agent::ToolResultPatch>
                    })
                }
            })
            .await;

        Ok(())
    }
}

fn map_agent_event(ui_tx: &mpsc::UnboundedSender<AgentUiEvent>, event: AgentEvent, show_thinking: bool, cwd: &str) {
    match event {
        AgentEvent::MessageStart { message } => {
            // User messages injected mid-run (drained follow-up / steer). Shell may skip if it
            // already echoed the prompt (idle submit or Ctrl+Enter interjection).
            if message.role() == "user" {
                let text = agent_user_text(&message);
                if !text.trim().is_empty() {
                    let _ = ui_tx.send(AgentUiEvent::UserPromptCommitted { text });
                }
            }
        }
        AgentEvent::MessageUpdate {
            assistant_message_event,
            ..
        } => match &*assistant_message_event {
            AssistantMessageEvent::TextDelta { delta, .. } => {
                let _ = ui_tx.send(AgentUiEvent::TextDelta(delta.clone()));
            }
            AssistantMessageEvent::ThinkingDelta { delta, .. } if show_thinking => {
                let _ = ui_tx.send(AgentUiEvent::ThinkingDelta(delta.clone()));
            }
            AssistantMessageEvent::Error { .. } => {
                // Final error text is on MessageEnd / TurnEnd via assistant.error_message.
            }
            _ => {}
        },
        AgentEvent::ToolExecutionStart {
            tool_call_id,
            tool_name,
            args,
            ..
        } => {
            let mut args_summary = serde_json::to_string(&args).unwrap_or_default();
            if tool_name == "shell_exec" {
                args_summary = elph_agent::normalize_shell_exec_args(&args_summary, cwd);
            }
            let _ = ui_tx.send(AgentUiEvent::ToolStart {
                id: tool_call_id,
                name: tool_name,
                args_summary,
                user_shell: false,
            });
        }
        AgentEvent::ToolExecutionUpdate {
            tool_call_id,
            partial_result,
            ..
        } => {
            let output = summarize_tool_result(&partial_result);
            if !output.is_empty() {
                let _ = ui_tx.send(AgentUiEvent::ToolUpdate {
                    id: tool_call_id,
                    output,
                });
            }
        }
        AgentEvent::ToolExecutionEnd {
            tool_call_id,
            is_error,
            result,
            ..
        } => {
            let _ = ui_tx.send(AgentUiEvent::ToolEnd {
                id: tool_call_id,
                is_error,
                output: summarize_tool_result(&result),
                details: result.details.clone(),
            });
        }
        AgentEvent::MessageEnd { message } => {
            emit_assistant_api_error(ui_tx, &message);
        }
        AgentEvent::TurnEnd { message, .. } => {
            // Backup path if MessageEnd was skipped; emit_assistant_api_error is idempotent-friendly.
            emit_assistant_api_error(ui_tx, &message);
        }
        AgentEvent::PlanConfirmationRequired { plan_id, plan_text } => {
            let _ = ui_tx.send(AgentUiEvent::PlanConfirmationRequired(PlanConfirmationRequest {
                plan_id,
                plan_text,
            }));
        }
        _ => {}
    }
}

/// Surface stream/API failures (401, 409, …) as a clear Status line for the TUI.
fn emit_assistant_api_error(ui_tx: &mpsc::UnboundedSender<AgentUiEvent>, message: &elph_agent::AgentMessage) {
    use crate::tui::api_error_display::format_user_facing_api_error;
    use elph_ai::Message;

    let Some(llm) = message.as_llm() else {
        return;
    };
    let Message::Assistant(assistant) = llm else {
        return;
    };
    let Some(raw) = assistant
        .error_message
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    else {
        return;
    };
    let text = format_user_facing_api_error(raw);
    let _ = ui_tx.send(AgentUiEvent::Status(text));
}

fn summarize_tool_result(result: &elph_agent::AgentToolResult) -> String {
    result
        .content
        .iter()
        .filter_map(|block| match block {
            elph_agent::ToolResultContent::Text(text) => Some(text.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("")
}

/// Map harness queue snapshot to numbered UI items (follow-ups first, then steer).
fn map_queue_update(update: &elph_agent::QueueUpdateEvent) -> Vec<QueuedPromptItem> {
    let mut items = Vec::with_capacity(update.follow_up.len() + update.steer.len());
    let mut seq = 1u32;
    for (kind_index, message) in update.follow_up.iter().enumerate() {
        let text = agent_user_text(message);
        if text.trim().is_empty() {
            continue;
        }
        items.push(QueuedPromptItem {
            seq,
            kind: QueuedPromptKind::FollowUp,
            kind_index,
            text,
        });
        seq = seq.saturating_add(1);
    }
    for (kind_index, message) in update.steer.iter().enumerate() {
        let text = agent_user_text(message);
        if text.trim().is_empty() {
            continue;
        }
        items.push(QueuedPromptItem {
            seq,
            kind: QueuedPromptKind::Steer,
            kind_index,
            text,
        });
        seq = seq.saturating_add(1);
    }
    items
}

pub(super) fn agent_message_preview(message: &elph_agent::AgentMessage) -> String {
    agent_user_text(message)
}

fn agent_user_text(message: &elph_agent::AgentMessage) -> String {
    use elph_ai::{ContentBlock, Message, UserContent};
    let Some(llm) = message.as_llm() else {
        return String::new();
    };
    match llm {
        Message::User { content, .. } => match content {
            UserContent::Text(text) => text.clone(),
            UserContent::Blocks(blocks) => blocks
                .iter()
                .filter_map(|block| match block {
                    ContentBlock::Text { text } => Some(text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join(""),
        },
        _ => String::new(),
    }
}
