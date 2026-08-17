//! Subagent spawn and control-plane API.

use std::sync::Arc;

use elph_ai::{Message, Model, UserContent};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use super::harness::SubagentHarness;
use super::harness::spawn_subagent_harness;
use super::id::MAX_NAME_ATTEMPTS;
use super::id::generate_agent_name;
use super::registry::{AgentRegistry, SubagentRecord};
use super::types::{SubagentBootstrap, SubagentInfo, SubagentLimits, SubagentStatus};
use crate::runtime::local_env::LocalExecutionEnv;
use crate::types::llm_message_to_agent;
use crate::types::{AgentEvent, AgentTool, StreamFn};

#[derive(Clone)]
pub struct SubagentSpawnConfig {
    pub env: Arc<LocalExecutionEnv>,
    pub model: Model,
    pub system_prompt: String,
    /// Full tool registry (native + MCP), including default-inactive MCP tools.
    pub base_tools: Vec<AgentTool>,
    /// Active tool names for the child harness (subset of `base_tools`).
    /// Empty means "all base_tools active" (legacy). Prefer an explicit parent active set.
    pub active_tool_names: Vec<String>,
    pub stream_fn: StreamFn,
    pub models: Arc<elph_ai::Models>,
    pub root_session_id: String,
    pub bootstrap: Option<SubagentBootstrap>,
}

pub type SubagentEventForwarder = Arc<dyn Fn(AgentEvent, &SubagentInfo) + Send + Sync>;

pub struct AgentControl {
    registry: Arc<AgentRegistry>,
    config: Mutex<SubagentSpawnConfig>,
    limits: SubagentLimits,
    depth: u32,
    parent_agent_path: String,
    event_forwarder: Mutex<Option<SubagentEventForwarder>>,
}

impl AgentControl {
    pub fn new(
        config: SubagentSpawnConfig,
        limits: SubagentLimits,
        depth: u32,
        registry: Arc<AgentRegistry>,
        parent_agent_path: impl Into<String>,
    ) -> Self {
        Self {
            registry,
            config: Mutex::new(config),
            limits,
            depth,
            parent_agent_path: parent_agent_path.into(),
            event_forwarder: Mutex::new(None),
        }
    }

    pub fn depth(&self) -> u32 {
        self.depth
    }

    pub fn registry(&self) -> Arc<AgentRegistry> {
        self.registry.clone()
    }

    /// Fetch a spawned subagent's harness by id (used for inspection/testing).
    pub async fn subagent_harness(&self, id: &str) -> Option<Arc<SubagentHarness>> {
        self.registry.get(id).await.map(|record| record.harness)
    }

    pub async fn set_event_forwarder(&self, forwarder: Option<SubagentEventForwarder>) {
        *self.event_forwarder.lock().await = forwarder;
    }

    pub async fn refresh_config(
        &self,
        system_prompt: String,
        model: Model,
        base_tools: Vec<AgentTool>,
        active_tool_names: Vec<String>,
    ) {
        let mut config = self.config.lock().await;
        config.system_prompt = system_prompt;
        config.model = model;
        config.base_tools = base_tools;
        config.active_tool_names = active_tool_names;
    }

    /// Keep the spawned subagent model in sync with the parent harness's current
    /// active model. Called by the harness whenever the live model changes, so a
    /// subagent spawned later inherits the active selection instead of the stale
    /// `defaultModel` captured at harness construction.
    pub async fn set_model(&self, model: Model) {
        self.config.lock().await.model = model;
    }

    pub async fn list_agents(&self, path_prefix: Option<&str>) -> Vec<SubagentInfo> {
        self.registry.list(path_prefix).await
    }

    /// Reconcile registry output state from the harness after a completed turn.
    async fn refresh_record_output(&self, agent_id: &str) {
        let Some(record) = self.registry.get(agent_id).await else {
            return;
        };
        let output = record.harness.output().await;
        let mut agents = self.registry.agents_mut().await;
        if let Some(record) = agents.get_mut(agent_id) {
            let mut info = record.info.clone();
            info.output = output;
            info.status = record.info.status;
            record.info = info;
        }
    }

    pub async fn spawn_agent(&self, task_name: impl Into<String>, message: Option<String>) -> Result<String, String> {
        if self.depth >= self.limits.max_depth {
            return Err(format!("Subagent depth limit ({}) reached", self.limits.max_depth));
        }
        if self.registry.count_active().await >= self.limits.max_concurrent {
            return Err(format!("Concurrent subagent limit ({}) reached", self.limits.max_concurrent));
        }

        let task_name = task_name.into();
        let (agent_id, agent_path) = {
            let mut reserved = None;
            for _ in 0..MAX_NAME_ATTEMPTS {
                let candidate = generate_agent_name();
                let candidate_path = format!("{}/{}", self.parent_agent_path, candidate);
                if self.registry.reserve_path(&candidate_path).await.is_ok() {
                    reserved = Some((candidate, candidate_path));
                    break;
                }
            }
            reserved.ok_or_else(|| "Failed to allocate a unique subagent name".to_string())?
        };

        let config = self.config.lock().await.clone();
        let bootstrap = config
            .bootstrap
            .clone()
            .ok_or_else(|| "Subagent bootstrap not configured — cannot spawn session-backed subagents".to_string())?;

        let child_depth = self.depth + 1;
        let child_control = Arc::new(AgentControl::new(
            config.clone(),
            self.limits.clone(),
            child_depth,
            self.registry.clone(),
            agent_path.clone(),
        ));
        if let Some(forwarder) = self.event_forwarder.lock().await.clone() {
            child_control.set_event_forwarder(Some(forwarder)).await;
        }

        // Add timeout protection for harness spawn (30 seconds)
        let harness = match tokio::time::timeout(
            tokio::time::Duration::from_secs(30),
            spawn_subagent_harness(
                &bootstrap,
                config.env.clone(),
                config.model.clone(),
                config.models.clone(),
                config.stream_fn.clone(),
                config.base_tools.clone(),
                config.active_tool_names.clone(),
                &config.root_session_id,
                &agent_id,
                &task_name,
                &agent_path,
                child_depth,
                self.limits.clone(),
                self.registry.clone(),
                child_control,
                config.system_prompt.clone(),
            ),
        )
        .await
        {
            Ok(Ok(h)) => h,
            Ok(Err(error)) => {
                self.registry.release_path(&agent_path).await;
                return Err(format!("Failed to spawn subagent harness: {error}"));
            }
            Err(_) => {
                self.registry.release_path(&agent_path).await;
                return Err("Subagent spawn timed out after 30 seconds".to_string());
            }
        };

        let id = harness.info().id.clone();
        let harness_for_forwarding = harness.clone();

        // Insert with Pending status first, then transition to Running when AgentStart fires
        let mut info = harness.info().clone();
        info.status = SubagentStatus::Pending;
        let record = SubagentRecord {
            info,
            harness: harness.clone(),
        };
        self.registry.insert(record).await;

        // Subscribe the event forwarder exactly once at spawn time — doing this
        // per-followup would accumulate duplicate subscribers over a long session.
        if let Some(forwarder) = self.event_forwarder.lock().await.clone() {
            harness_for_forwarding.forward_events(forwarder).await;
        }

        if let Some(text) = message {
            // Add timeout protection for initial followup task (60 seconds)
            match tokio::time::timeout(tokio::time::Duration::from_secs(60), self.followup_task(&id, text)).await {
                Ok(Ok(())) => {}
                Ok(Err(e)) => {
                    log::error!("Initial subagent followup failed: {}", e);
                    self.registry.set_status(&id, SubagentStatus::Error).await;
                    return Err(format!("Initial subagent task failed: {e}"));
                }
                Err(_) => {
                    log::error!("Initial subagent followup timed out after 60 seconds");
                    self.registry.set_status(&id, SubagentStatus::Error).await;
                    return Err("Initial subagent task timed out after 60 seconds".to_string());
                }
            }
        }

        Ok(id)
    }

    pub async fn send_message(&self, agent_id: &str, message: String) -> Result<(), String> {
        let record = self
            .registry
            .get(agent_id)
            .await
            .ok_or_else(|| format!("Unknown agent: {agent_id}"))?;
        record
            .harness
            .harness()
            .queue_user_message(llm_message_to_agent(Message::User {
                content: UserContent::Text(message),
                timestamp: now_ms(),
            }))
            .await
            .map_err(|e| e.to_string())
    }

    /// Run a turn on the subagent and return the final assistant text (or a
    /// readable status when the agent produced no final message).
    ///
    /// The turn runs in a background task; callers that need the result should
    /// use [`Self::wait_agent_for_output`] to block on completion.
    pub async fn followup_task(&self, agent_id: &str, message: String) -> Result<(), String> {
        let record = self
            .registry
            .get(agent_id)
            .await
            .ok_or_else(|| format!("Unknown agent: {agent_id}"))?;

        self.registry.set_status(agent_id, SubagentStatus::Running).await;

        let harness = record.harness.clone();
        let id = agent_id.to_string();
        let registry = self.registry.clone();
        let info = record.info.clone();

        // Track the dispatched turn synchronously so `wait_agent` callers always
        // observe at least one in-flight turn (no start-of-turn race). The guard
        // releases the slot even if the task panics or is cancelled.
        let guard = harness.turn_guard();
        tokio::spawn(async move {
            let _guard = guard;
            let result = harness.followup(message).await;
            let status = if result.is_ok() {
                SubagentStatus::Done
            } else {
                SubagentStatus::Error
            };
            registry.set_status(&id, status).await;
            if let Some(graph) = harness.harness().agent_graph() {
                let _ = graph.close_edge(&info.parent_session_id, &info.session_id).await;
            }
        });

        Ok(())
    }

    /// Block until the subagent's current turn completes and return its final
    /// assistant text. Falls back to a readable placeholder when the agent
    /// produced no output (so tool results always carry observable content).
    pub async fn wait_agent_for_output(&self, agent_id: &str) -> Result<String, String> {
        self.wait_agent_cancellable_for_output(agent_id, None).await
    }

    pub async fn wait_agent(&self, agent_id: &str) -> Result<(), String> {
        self.wait_agent_cancellable(agent_id, None).await
    }

    pub async fn wait_agent_cancellable(
        &self,
        agent_id: &str,
        signal: Option<&CancellationToken>,
    ) -> Result<(), String> {
        let _ = self.wait_agent_cancellable_for_output(agent_id, signal).await?;
        Ok(())
    }

    pub async fn wait_agent_cancellable_for_output(
        &self,
        agent_id: &str,
        signal: Option<&CancellationToken>,
    ) -> Result<String, String> {
        let record = self
            .registry
            .get(agent_id)
            .await
            .ok_or_else(|| format!("Unknown agent: {agent_id}"))?;

        // Wait for any dispatched-but-not-yet-started turn to begin and finish.
        let result = if let Some(token) = signal {
            tokio::select! {
                () = record.harness.wait_for_turn() => { record.harness.wait_idle().await }
                () = token.cancelled() => {
                    let _ = record.harness.harness().cancel_active_run().await;
                    Err("Operation aborted".to_string())
                }
            }
        } else {
            record.harness.wait_for_turn().await;
            record.harness.wait_idle().await
        };

        let upstream_result = match result {
            Ok(()) => {
                self.registry.set_status(agent_id, SubagentStatus::Idle).await;
                Ok(())
            }
            Err(error) => {
                self.registry.set_status(agent_id, SubagentStatus::Error).await;
                Err(error)
            }
        };
        let output_text = record.harness.last_output().await;
        self.refresh_record_output(agent_id).await;

        upstream_result?;
        // Prefer the exact final assistant text; fall back to the registry summary
        // (never empty — it carries the persistent log path when no text exists).
        let output = self
            .registry
            .get(agent_id)
            .await
            .map(|record| record.info.output.summary())
            .unwrap_or_default();
        if output_text.trim().is_empty() {
            Ok(output)
        } else {
            Ok(output_text)
        }
    }

    /// Abort every subagent that is still pending or running.
    pub async fn abort_all_running(&self) {
        for record in self.registry.running_records().await {
            let id = record.info.id.clone();
            let _ = record.harness.harness().cancel_active_run().await;
            self.registry.set_status(&id, SubagentStatus::Error).await;
        }
    }

    /// Health check: mark subagents stuck in Pending state for too long as Error.
    /// This prevents infinite spinner when AgentStart never fires.
    pub async fn health_check_stuck_pending(&self, timeout_secs: u64) {
        let timeout = std::time::Duration::from_secs(timeout_secs);
        let stuck_agents = self.registry.stuck_pending_agents(timeout).await;
        for agent_id in stuck_agents {
            log::warn!(
                "Subagent {} stuck in Pending state for {:?}, marking as Error",
                agent_id,
                timeout
            );
            self.registry.set_status(&agent_id, SubagentStatus::Error).await;
        }
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
