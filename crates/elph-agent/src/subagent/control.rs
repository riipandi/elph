//! Subagent spawn and control-plane API.

use std::sync::Arc;

use elph_ai::{Message, Model, UserContent};
use tokio::sync::Mutex;

use super::registry::{AgentRegistry, SubagentRecord};
use super::types::{SubagentInfo, SubagentLimits, SubagentStatus};
use crate::agent::{Agent, AgentOptions, PartialAgentState};
use crate::env::LocalExecutionEnv;
use crate::session::id::create_tsid;
use crate::types::AgentEvent;
use crate::types::{AgentTool, StreamFn, llm_message_to_agent};

#[derive(Clone)]
pub struct SubagentSpawnConfig {
    pub env: Arc<LocalExecutionEnv>,
    pub model: Model,
    pub system_prompt: String,
    pub tools: Vec<AgentTool>,
    pub stream_fn: StreamFn,
    pub parent_session_id: String,
}

pub type SubagentEventForwarder = Arc<dyn Fn(AgentEvent) + Send + Sync>;

pub struct AgentControl {
    registry: Arc<AgentRegistry>,
    config: Mutex<SubagentSpawnConfig>,
    limits: SubagentLimits,
    depth: u32,
    event_forwarder: Mutex<Option<SubagentEventForwarder>>,
}

impl AgentControl {
    pub fn new(config: SubagentSpawnConfig, limits: SubagentLimits, depth: u32) -> Self {
        Self {
            registry: Arc::new(AgentRegistry::new()),
            config: Mutex::new(config),
            limits,
            depth,
            event_forwarder: Mutex::new(None),
        }
    }

    pub async fn set_event_forwarder(&self, forwarder: Option<SubagentEventForwarder>) {
        *self.event_forwarder.lock().await = forwarder;
    }

    pub async fn refresh_config(&self, system_prompt: String, model: Model, tools: Vec<AgentTool>) {
        let mut config = self.config.lock().await;
        config.system_prompt = system_prompt;
        config.model = model;
        config.tools = tools;
    }

    pub fn registry(&self) -> Arc<AgentRegistry> {
        self.registry.clone()
    }

    pub async fn list_agents(&self) -> Vec<SubagentInfo> {
        self.registry.list().await
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
        let id = format!("agent_{}", create_tsid());
        let config = self.config.lock().await.clone();

        let child = Agent::new(AgentOptions {
            initial_state: Some(PartialAgentState {
                system_prompt: Some(config.system_prompt.clone()),
                model: Some(config.model.clone()),
                tools: Some(config.tools.clone()),
                ..Default::default()
            }),
            stream_fn: Some(config.stream_fn.clone()),
            session_id: Some(format!("{}:{}", config.parent_session_id, id)),
            ..Default::default()
        });

        let record = SubagentRecord {
            info: SubagentInfo {
                id: id.clone(),
                task_name,
                status: SubagentStatus::Pending,
                parent_id: Some(config.parent_session_id.clone()),
            },
            agent: Arc::new(child),
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
        record.agent.follow_up(llm_message_to_agent(Message::User {
            content: UserContent::Text(message),
            timestamp: now_ms(),
        }));
        Ok(())
    }

    pub async fn followup_task(&self, agent_id: &str, message: String) -> Result<(), String> {
        let record = self
            .registry
            .get(agent_id)
            .await
            .ok_or_else(|| format!("Unknown agent: {agent_id}"))?;
        self.registry.set_status(agent_id, SubagentStatus::Running).await;
        let agent = record.agent.clone();
        let id = agent_id.to_string();
        let registry = self.registry.clone();
        let forwarder = self.event_forwarder.lock().await.clone();
        if let Some(forwarder) = forwarder {
            let forwarder = forwarder.clone();
            agent
                .subscribe(Arc::new(move |event, _| {
                    let forwarder = forwarder.clone();
                    Box::pin(async move {
                        forwarder(event);
                    })
                }))
                .await;
        }
        tokio::spawn(async move {
            let result = agent.prompt_text(message, None).await;
            agent.wait_for_idle().await;
            let status = if result.is_ok() {
                SubagentStatus::Done
            } else {
                SubagentStatus::Error
            };
            registry.set_status(&id, status).await;
        });
        Ok(())
    }

    pub async fn wait_agent(&self, agent_id: &str) -> Result<(), String> {
        let record = self
            .registry
            .get(agent_id)
            .await
            .ok_or_else(|| format!("Unknown agent: {agent_id}"))?;
        record.agent.wait_for_idle().await;
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
