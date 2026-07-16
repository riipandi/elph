//! Stateful coding session wrapping `AgentHarness`.

mod wiring;

use crate::types::AgentMode;
use anyhow::Result;
use elph_agent::{AgentHarness, AgentHarnessErrorCode};
use elph_agent::{GoalRuntime, McpToolRegistry, PlanConfirmationChoice, SessionDirStorage};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{Mutex, mpsc};

use super::events::AgentUiEvent;
use super::model_registry::ModelSelection;
use super::resource_loader::load_resources;
use super::session_manager::SessionManager;
use super::tool_policy::{AgentModePolicy, to_agent_thinking};
use super::tools_catalog::reconcile_harness_tools;
use crate::platform::Paths;
use elph_agent::parse_command_args;
use std::path::Path;

/// Constructor inputs for [`CodingAgentSession::new`] (avoids a long positional arg list).
pub struct CodingAgentSessionParams {
    pub harness: Arc<AgentHarness<SessionDirStorage>>,
    pub session_manager: SessionManager,
    pub session_id: String,
    pub selection: ModelSelection,
    pub agent_mode: AgentMode,
    pub mode_state: Arc<Mutex<AgentMode>>,
    pub show_thinking: bool,
    pub goal_runtime: Arc<GoalRuntime>,
    pub mcp_registry: Option<Arc<McpToolRegistry>>,
    pub ui_tx: mpsc::UnboundedSender<AgentUiEvent>,
}

pub struct CodingAgentSession {
    harness: Arc<AgentHarness<SessionDirStorage>>,
    session_manager: SessionManager,
    session_id: String,
    selection: ModelSelection,
    policy: Arc<Mutex<AgentModePolicy>>,
    mode_state: Arc<Mutex<AgentMode>>,
    ui_tx: mpsc::UnboundedSender<AgentUiEvent>,
    show_thinking: bool,
    goal_runtime: Arc<GoalRuntime>,
    mcp_registry: Option<Arc<McpToolRegistry>>,
    /// Serializes harness turns so only one prompt/template/compact runs at a time.
    turn_gate: Arc<Mutex<()>>,
    /// Serializes agent-mode reconciliation (Ctrl+A rapid cycling).
    mode_gate: Arc<Mutex<()>>,
}

impl CodingAgentSession {
    pub async fn new(params: CodingAgentSessionParams) -> Result<Self> {
        let CodingAgentSessionParams {
            harness,
            session_manager,
            session_id,
            selection,
            agent_mode,
            mode_state,
            show_thinking,
            goal_runtime,
            mcp_registry,
            ui_tx,
        } = params;
        let mut policy = AgentModePolicy::new(agent_mode);
        if let Some(reg) = mcp_registry.clone() {
            policy = policy.with_mcp_registry(reg);
        }
        let session = Self {
            harness: harness.clone(),
            session_manager,
            session_id,
            selection,
            policy: Arc::new(Mutex::new(policy)),
            mode_state,
            ui_tx: ui_tx.clone(),
            show_thinking,
            goal_runtime,
            mcp_registry,
            turn_gate: Arc::new(Mutex::new(())),
            mode_gate: Arc::new(Mutex::new(())),
        };
        session.wire_harness(ui_tx).await?;
        session.apply_agent_mode(agent_mode).await?;
        Ok(session)
    }

    pub fn mode_state(&self) -> Arc<Mutex<AgentMode>> {
        self.mode_state.clone()
    }

    /// Re-apply tool permissions after MCP hot-reload or tool set changes.
    pub async fn reconcile_tool_surface(&self) -> Result<()> {
        let mode = *self.mode_state.lock().await;
        self.apply_agent_mode(mode).await
    }

    pub fn mcp_registry(&self) -> Option<Arc<McpToolRegistry>> {
        self.mcp_registry.clone()
    }

    pub fn ui_event_sender(&self) -> mpsc::UnboundedSender<AgentUiEvent> {
        self.ui_tx.clone()
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

    pub fn context_window(&self) -> u32 {
        self.selection.model.context_window
    }

    pub fn supports_image_input(&self) -> bool {
        self.selection.model.input.iter().any(|cap| cap == "image")
    }

    pub fn goal_runtime(&self) -> Arc<GoalRuntime> {
        self.goal_runtime.clone()
    }

    pub async fn set_agent_mode(&self, mode: AgentMode) -> Result<()> {
        let _guard = self.mode_gate.lock().await;
        *self.mode_state.lock().await = mode;
        self.policy.lock().await.set_mode(mode);
        // Wait for any in-flight turn before reconciling tools (avoids mid-turn mode races).
        let _turn_guard = self.turn_gate.lock().await;
        self.apply_agent_mode(mode).await
    }

    pub async fn set_thinking_level(&self, level: crate::types::ThinkingLevel) -> Result<()> {
        self.harness
            .set_thinking_level(to_agent_thinking(level))
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        Ok(())
    }

    pub async fn submit_prompt(&self, text: String, steer: bool) -> Result<()> {
        let _guard = self.turn_gate.lock().await;
        let started = Instant::now();
        let result = if steer {
            self.harness.steer(text, None).await.map(|_| ())
        } else {
            self.harness.prompt(text, None).await.map(|_| ())
        };
        match &result {
            Ok(()) => self.finish_ui_turn(started).await,
            Err(err) if err.code == AgentHarnessErrorCode::Busy => {
                let _ = self.ui_tx.send(AgentUiEvent::Status(format!("Error: {err}")));
            }
            Err(err) => {
                self.finish_ui_turn(started).await;
                let _ = self.ui_tx.send(AgentUiEvent::Status(format!("Error: {err}")));
            }
        }
        result.map_err(|err| anyhow::anyhow!("{err}"))
    }

    pub async fn abort(&self) -> Result<()> {
        self.harness
            .abort()
            .await
            .map(|_| ())
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn compact(&self) -> Result<()> {
        let _guard = self.turn_gate.lock().await;
        let started = Instant::now();
        let result = self.harness.compact(None).await.map(|_| ());
        self.finish_ui_turn(started).await;
        if let Err(err) = &result {
            let _ = self.ui_tx.send(AgentUiEvent::Status(format!("Compact failed: {err}")));
        }
        result.map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn reload_resources(&self, paths: &Paths, cwd: &Path) -> Result<()> {
        let env = self.harness.env();
        let resources = load_resources(paths, cwd, env.as_ref()).await;
        self.harness
            .set_resources(resources)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn prompt_from_template(&self, name: &str, args: &str) -> Result<()> {
        let _guard = self.turn_gate.lock().await;
        let started = Instant::now();
        let parsed = parse_command_args(args);
        let result = self.harness.prompt_from_template(name, &parsed).await.map(|_| ());
        match &result {
            Ok(()) => self.finish_ui_turn(started).await,
            Err(err) if err.code == AgentHarnessErrorCode::Busy => {
                let _ = self.ui_tx.send(AgentUiEvent::Status(format!("Template error: {err}")));
            }
            Err(err) => {
                self.finish_ui_turn(started).await;
                let _ = self.ui_tx.send(AgentUiEvent::Status(format!("Template error: {err}")));
            }
        }
        result.map_err(|e| anyhow::anyhow!("{e}"))
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
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        // Implementing a plan exits harness Plan mode — restore Build tool surface.
        if matches!(
            choice,
            PlanConfirmationChoice::Implement | PlanConfirmationChoice::ImplementFresh
        ) {
            *self.mode_state.lock().await = AgentMode::Build;
            self.policy.lock().await.set_mode(AgentMode::Build);
            self.apply_agent_mode(AgentMode::Build).await?;
        }
        Ok(())
    }

    async fn apply_agent_mode(&self, mode: AgentMode) -> Result<()> {
        reconcile_harness_tools(&self.harness, mode, self.mcp_registry.as_deref()).await
    }

    async fn finish_ui_turn(&self, started: Instant) {
        let _ = self.harness.wait_for_idle().await;
        let _ = self.ui_tx.send(AgentUiEvent::RunCompleted {
            elapsed_secs: started.elapsed().as_secs_f64(),
        });
    }
}
