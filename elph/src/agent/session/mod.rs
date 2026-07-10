//! Stateful coding session wrapping `AgentHarness`.

mod wiring;

use anyhow::Result;
use elph_agent::{AgentHarness, CollaborationMode, GoalRuntime, PlanConfirmationChoice, SessionDirStorage};
use elph_tui::AgentMode;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{Mutex, mpsc};

use super::events::AgentUiEvent;
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
}
