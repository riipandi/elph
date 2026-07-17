//! Harness event wiring and UI event mapping.

use anyhow::Result;
use elph_agent::{
    AgentEvent, AgentHarnessEvent, AgentHarnessOwnEvent, SubagentEventForwarder, SubagentInfo, ToolCallEvent,
    ToolCallHookResult,
};
use elph_ai::AssistantMessageEvent;
use std::sync::Arc;
use tokio::sync::mpsc;

use super::CodingAgentSession;

use crate::agent::events::{AgentUiEvent, PlanConfirmationRequest};

impl CodingAgentSession {
    pub(super) async fn wire_harness(&self, ui_tx: mpsc::UnboundedSender<AgentUiEvent>) -> Result<()> {
        let harness = self.harness.clone();
        let policy = self.policy.clone();
        let show_thinking = self.show_thinking;

        harness
            .on_tool_call({
                let ui_tx = ui_tx.clone();
                let policy = Arc::clone(&policy);
                move |event: &ToolCallEvent| {
                    let ui_tx = ui_tx.clone();
                    let policy = Arc::clone(&policy);
                    let tool_call_id = event.tool_call_id.clone();
                    let tool_name = event.tool_name.clone();
                    let args_summary = serde_json::to_string(&event.input).unwrap_or_default();
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
                move |event, _| {
                    let ui_tx = ui_tx.clone();
                    Box::pin(async move {
                        if let AgentHarnessEvent::Agent(agent_event) = event {
                            map_agent_event(&ui_tx, agent_event, show_thinking);
                        } else if let AgentHarnessEvent::Own(AgentHarnessOwnEvent::QueueUpdate(update)) = event {
                            let steering: Vec<String> = update
                                .steer
                                .iter()
                                .filter_map(|m| {
                                    m.as_llm().and_then(|msg| match msg {
                                        elph_ai::Message::User { content, .. } => Some(format!("{content:?}")),
                                        _ => None,
                                    })
                                })
                                .collect();
                            let _ = steering;
                        }
                    })
                }
            })
            .await;

        let forwarder: SubagentEventForwarder = Arc::new({
            let ui_tx = ui_tx.clone();
            move |event, info: &SubagentInfo| {
                use crate::agent::SubagentUiPhase;
                match event {
                    // Lifecycle: clear status words (not every token/tool delta).
                    AgentEvent::AgentStart => {
                        let _ = ui_tx.send(AgentUiEvent::SubagentStatus {
                            agent_id: info.id.clone(),
                            agent_path: info.agent_path.clone(),
                            task_name: info.task_name.clone(),
                            phase: SubagentUiPhase::Running,
                            message: String::new(),
                        });
                    }
                    AgentEvent::AgentEnd { .. } => {
                        let _ = ui_tx.send(AgentUiEvent::SubagentStatus {
                            agent_id: info.id.clone(),
                            agent_path: info.agent_path.clone(),
                            task_name: info.task_name.clone(),
                            phase: SubagentUiPhase::Done,
                            message: String::new(),
                        });
                    }
                    // Tool activity: upsert running row with human verb (low noise via upsert).
                    AgentEvent::ToolExecutionStart { tool_name, .. } => {
                        let _ = ui_tx.send(AgentUiEvent::SubagentStatus {
                            agent_id: info.id.clone(),
                            agent_path: info.agent_path.clone(),
                            task_name: info.task_name.clone(),
                            phase: SubagentUiPhase::Running,
                            message: format!("tool:{tool_name}"),
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
                        });
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

        Ok(())
    }
}

fn map_agent_event(ui_tx: &mpsc::UnboundedSender<AgentUiEvent>, event: AgentEvent, show_thinking: bool) {
    match event {
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
            let args_summary = serde_json::to_string(&args).unwrap_or_default();
            let _ = ui_tx.send(AgentUiEvent::ToolStart {
                id: tool_call_id,
                name: tool_name,
                args_summary,
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
