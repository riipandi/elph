//! Stateful coding session wrapping `AgentHarness`.

use anyhow::Result;
use elph_agent::{
    AgentEvent, AgentHarness, AgentHarnessEvent, AgentHarnessOwnEvent, CollaborationMode, GoalRuntime,
    PlanConfirmationChoice, SessionDirStorage, SubagentEventForwarder, SubagentInfo, ToolCallEvent, ToolCallHookResult,
};
use elph_ai::AssistantMessageEvent;
use elph_tui::AgentMode;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{Mutex, mpsc};

use super::events::{AgentUiEvent, PlanConfirmationRequest};
use super::model_registry::ModelSelection;
use super::session_manager::SessionManager;
use super::tool_policy::{AgentModePolicy, to_agent_thinking};

pub struct CodingAgentSession {
    harness: Arc<AgentHarness<SessionDirStorage>>,
    session_manager: SessionManager,
    session_id: String,
    selection: ModelSelection,
    policy: Arc<Mutex<AgentModePolicy>>,
    ui_tx: mpsc::UnboundedSender<AgentUiEvent>,
    show_thinking: bool,
    goal_runtime: Arc<GoalRuntime>,
}

impl CodingAgentSession {
    pub async fn new(
        harness: Arc<AgentHarness<SessionDirStorage>>,
        session_manager: SessionManager,
        session_id: String,
        selection: ModelSelection,
        agent_mode: AgentMode,
        show_thinking: bool,
        goal_runtime: Arc<GoalRuntime>,
    ) -> Result<(Self, mpsc::UnboundedReceiver<AgentUiEvent>)> {
        let (ui_tx, ui_rx) = mpsc::unbounded_channel();
        let session = Self {
            harness: harness.clone(),
            session_manager,
            session_id,
            selection,
            policy: Arc::new(Mutex::new(AgentModePolicy::new(agent_mode))),
            ui_tx: ui_tx.clone(),
            show_thinking,
            goal_runtime,
        };
        session.wire_harness(ui_tx).await?;
        session.apply_agent_mode(agent_mode).await?;
        Ok((session, ui_rx))
    }

    pub fn harness(&self) -> Arc<AgentHarness<SessionDirStorage>> {
        self.harness.clone()
    }

    pub fn session_manager(&self) -> &SessionManager {
        &self.session_manager
    }

    pub fn model_display(&self) -> String {
        format!(
            "{} [{}/{}]",
            self.selection.display_name, self.selection.provider, self.selection.model_id
        )
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    pub fn goal_runtime(&self) -> Arc<GoalRuntime> {
        self.goal_runtime.clone()
    }

    pub async fn set_agent_mode(&self, mode: AgentMode) -> Result<()> {
        self.policy.lock().await.set_mode(mode);
        self.apply_agent_mode(mode).await
    }

    pub async fn set_thinking_level(&self, level: elph_tui::ThinkingLevel) -> Result<()> {
        self.harness
            .set_thinking_level(to_agent_thinking(level))
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        Ok(())
    }

    pub async fn submit_prompt(&self, text: String, steer: bool) -> Result<()> {
        let start = Instant::now();
        let _ = self.ui_tx.send(AgentUiEvent::Status("Thinking…".into()));
        let result = if steer {
            self.harness.steer(text, None).await.map(|_| ())
        } else {
            self.harness.prompt(text, None).await.map(|_| ())
        };
        let elapsed_secs = start.elapsed().as_secs_f64();
        let _ = self.harness.wait_for_idle().await;
        let _ = self.ui_tx.send(AgentUiEvent::RunCompleted { elapsed_secs });
        match result {
            Ok(_) => Ok(()),
            Err(err) => {
                let _ = self.ui_tx.send(AgentUiEvent::Status(format!("Error: {err}")));
                Err(anyhow::anyhow!("{err}"))
            }
        }
    }

    pub async fn abort(&self) -> Result<()> {
        self.harness
            .abort()
            .await
            .map(|_| ())
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn compact(&self) -> Result<()> {
        self.harness
            .compact(None)
            .await
            .map(|_| ())
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn set_model_from_value(&self, value: &str) -> Result<String> {
        let model = super::overlays::resolve_model_from_value(value)?;
        self.harness
            .set_model(model.clone())
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        Ok(format!("{} [{}]", model.name, model.provider))
    }

    pub async fn navigate_tree_to(&self, entry_id: &str) -> Result<()> {
        self.harness
            .navigate_tree(entry_id, None)
            .await
            .map(|_| ())
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn branch_entries(&self) -> Result<Vec<elph_agent::SessionTreeEntry>> {
        self.harness
            .session_branch_entries()
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn resolve_plan(&self, choice: PlanConfirmationChoice) -> Result<()> {
        self.harness
            .resolve_plan_confirmation(choice)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    async fn apply_agent_mode(&self, mode: AgentMode) -> Result<()> {
        match mode {
            AgentMode::Plan => {
                self.harness
                    .enter_plan_mode()
                    .await
                    .map_err(|e| anyhow::anyhow!("{e}"))?;
            }
            AgentMode::Ask => {
                self.harness
                    .set_collaboration_mode(CollaborationMode::Default)
                    .await
                    .map_err(|e| anyhow::anyhow!("{e}"))?;
                self.harness
                    .set_active_tools(AgentModePolicy::read_only_tool_names())
                    .await
                    .map_err(|e| anyhow::anyhow!("{e}"))?;
            }
            AgentMode::Build | AgentMode::Brave => {
                self.harness
                    .set_collaboration_mode(CollaborationMode::Default)
                    .await
                    .map_err(|e| anyhow::anyhow!("{e}"))?;
                let tools: Vec<String> = self
                    .harness
                    .get_tools()
                    .await
                    .into_iter()
                    .map(|t| t.name().to_string())
                    .collect();
                self.harness
                    .set_active_tools(tools)
                    .await
                    .map_err(|e| anyhow::anyhow!("{e}"))?;
            }
        }
        Ok(())
    }

    async fn wire_harness(&self, ui_tx: mpsc::UnboundedSender<AgentUiEvent>) -> Result<()> {
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
                if let AgentEvent::ToolExecutionStart { tool_name, .. } = &event {
                    let _ = ui_tx.send(AgentUiEvent::SubagentStatus {
                        agent_id: info.id.clone(),
                        agent_path: info.agent_path.clone(),
                        message: format!("tool: {tool_name}"),
                    });
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
        AgentEvent::PlanConfirmationRequired { plan_id, plan_text } => {
            let _ = ui_tx.send(AgentUiEvent::PlanConfirmationRequired(PlanConfirmationRequest {
                plan_id,
                plan_text,
            }));
        }
        _ => {}
    }
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
