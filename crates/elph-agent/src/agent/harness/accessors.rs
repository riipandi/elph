//! Agent harness read-only accessors.

use std::sync::Arc;

use elph_ai::Model;

use crate::agent::harness::hooks::AgentHarnessEvent;
use crate::agent::harness::types::{
    AgentHarnessPhase, AgentHarnessResources, AgentHarnessStreamOptions, clone_stream_options,
};
use crate::agent::subagent::AgentControl;
use crate::collaboration::CollaborationMode;
use crate::session::types::{HasSessionId, SessionStorage, SessionTreeEntry};
use crate::types::{AgentMessage, AgentThinkingLevel, AgentTool, QueueMode};

use super::helpers::session_error;
use super::{AgentHarness, HarnessOpResult};

impl<S> AgentHarness<S>
where
    S: SessionStorage + Clone + Send + Sync + 'static,
    S::Metadata: HasSessionId + Send + Sync,
{
    pub fn env(&self) -> Arc<crate::runtime::local_env::LocalExecutionEnv> {
        self.shared.env.clone()
    }

    pub fn models(&self) -> Arc<elph_ai::Models> {
        self.shared.models.clone()
    }

    pub async fn session_entries(&self) -> Vec<SessionTreeEntry> {
        self.shared.session.lock().await.entries().await
    }

    pub async fn session_branch_entries(&self) -> HarnessOpResult<Vec<SessionTreeEntry>> {
        self.shared
            .session
            .lock()
            .await
            .branch(None)
            .await
            .map_err(session_error)
    }

    pub async fn phase(&self) -> AgentHarnessPhase {
        *self.shared.phase.lock().await
    }

    pub async fn get_model(&self) -> Model {
        self.shared.model.lock().await.clone()
    }

    pub async fn get_thinking_level(&self) -> AgentThinkingLevel {
        *self.shared.thinking_level.lock().await
    }

    pub async fn get_steering_mode(&self) -> QueueMode {
        *self.shared.steering_queue_mode.lock().await
    }

    pub async fn get_follow_up_mode(&self) -> QueueMode {
        *self.shared.follow_up_queue_mode.lock().await
    }

    pub async fn get_tools(&self) -> Vec<AgentTool> {
        self.shared.tools.lock().await.values().cloned().collect()
    }

    pub async fn get_active_tools(&self) -> Vec<AgentTool> {
        let tools = self.shared.tools.lock().await;
        let names = self.shared.active_tool_names.lock().await;
        names.iter().filter_map(|name| tools.get(name).cloned()).collect()
    }

    pub async fn collaboration_mode(&self) -> CollaborationMode {
        *self.shared.collaboration_mode.lock().await
    }

    pub async fn agent_control(&self) -> Arc<AgentControl> {
        self.shared.agent_control.lock().await.clone()
    }

    pub fn agent_graph(&self) -> Option<Arc<crate::agent::subagent::AgentGraphStore>> {
        self.shared
            .subagent_bootstrap
            .as_ref()
            .and_then(|b| b.agent_graph.clone())
    }

    pub async fn refresh_subagent_config(&self, system_prompt: String, model: Model) {
        let active_tools = self.shared.active_tool_names.lock().await.clone();
        let tools_map = self.shared.tools.lock().await;
        let base_tools: Vec<AgentTool> = active_tools
            .iter()
            .filter_map(|name| tools_map.get(name).cloned())
            .filter(|tool| !crate::collaboration::is_collaboration_tool(tool.name()))
            .collect();
        drop(tools_map);
        self.shared
            .agent_control
            .lock()
            .await
            .refresh_config(system_prompt, model, base_tools)
            .await;
    }

    pub async fn queue_user_message(&self, message: AgentMessage) -> HarnessOpResult<()> {
        self.shared.next_turn_queue.lock().await.push(message);
        Ok(())
    }

    pub async fn subscribe_agent_events<F>(&self, callback: Arc<F>)
    where
        F: Fn(crate::types::AgentEvent) + Send + Sync + 'static,
    {
        self.subscribe(move |event, _| {
            let callback = callback.clone();
            Box::pin(async move {
                if let AgentHarnessEvent::Agent(agent_event) = event {
                    callback(agent_event);
                }
            })
        })
        .await;
    }

    pub async fn get_resources(&self) -> AgentHarnessResources {
        self.shared.resources.lock().await.clone()
    }

    pub async fn get_stream_options(&self) -> AgentHarnessStreamOptions {
        let guard = self.shared.stream_options.lock().await;
        clone_stream_options(&guard)
    }
}
