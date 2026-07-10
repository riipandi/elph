//! Subagent spawn and control-plane API.

use std::sync::Arc;

use elph_ai::{Message, Model, UserContent};
use tokio::sync::Mutex;

use super::harness::spawn_subagent_harness;
use super::id::{MAX_NAME_ATTEMPTS, generate_agent_name};
use super::registry::{AgentRegistry, SubagentRecord};
use super::types::{SubagentBootstrap, SubagentInfo, SubagentLimits, SubagentStatus};
use crate::env::LocalExecutionEnv;
use crate::types::{AgentEvent, AgentTool, StreamFn, llm_message_to_agent};

#[derive(Clone)]
pub struct SubagentSpawnConfig {
    pub env: Arc<LocalExecutionEnv>,
    pub model: Model,
    pub system_prompt: String,
    pub base_tools: Vec<AgentTool>,
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

    pub async fn set_event_forwarder(&self, forwarder: Option<SubagentEventForwarder>) {
        *self.event_forwarder.lock().await = forwarder;
    }

    pub async fn refresh_config(&self, system_prompt: String, model: Model, base_tools: Vec<AgentTool>) {
        let mut config = self.config.lock().await;
        config.system_prompt = system_prompt;
        config.model = model;
        config.base_tools = base_tools;
    }

    pub async fn list_agents(&self, path_prefix: Option<&str>) -> Vec<SubagentInfo> {
        self.registry.list(path_prefix).await
    }

    pub async fn spawn_agent(&self, task_name: impl Into<String>, message: Option<String>) -> Result<String, String> {
        if self.depth >= self.limits.max_depth {
            return Err(format!("Subagent depth limit ({}) reached", self.limits.max_depth));
        }
        if self.registry.count_active().await >= self.limits.max_concurrent {
            return Err(format!(
                "Concurrent subagent limit ({}) reached",
                self.limits.max_concurrent
            ));
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

        let harness = match spawn_subagent_harness(
            &bootstrap,
            config.env.clone(),
            config.model.clone(),
            config.models.clone(),
            config.stream_fn.clone(),
            config.base_tools.clone(),
            &config.root_session_id,
            &agent_id,
            &task_name,
            &agent_path,
            child_depth,
            self.limits.clone(),
            self.registry.clone(),
            child_control,
            config.system_prompt.clone(),
        )
        .await
        {
            Ok(h) => h,
            Err(error) => {
                self.registry.release_path(&agent_path).await;
                return Err(error);
            }
        };

        let id = harness.info().id.clone();
        let record = SubagentRecord {
            info: harness.info().clone(),
            harness,
        };
        self.registry.insert(record).await;

        if let Some(text) = message {
            self.followup_task(&id, text).await?;
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
        let forwarder = self.event_forwarder.lock().await.clone();
        let info = record.info.clone();

        if let Some(forwarder) = forwarder {
            let info = info.clone();
            harness
                .harness()
                .subscribe_agent_events(Arc::new(move |event| {
                    forwarder(event, &info);
                }))
                .await;
        }

        tokio::spawn(async move {
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

    pub async fn wait_agent(&self, agent_id: &str) -> Result<(), String> {
        let record = self
            .registry
            .get(agent_id)
            .await
            .ok_or_else(|| format!("Unknown agent: {agent_id}"))?;
        record.harness.wait_idle().await?;
        self.registry.set_status(agent_id, SubagentStatus::Idle).await;
        Ok(())
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
