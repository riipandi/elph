//! Turn state snapshot creation.

use crate::agent::harness::types::{SystemPrompt, clone_stream_options};
use crate::collaboration::{CollaborationMode, plan_mode_system_prompt};
use crate::session::types::HasSessionId;
use crate::types::AgentTool;

use super::super::helpers::session_error;
use super::super::{AgentHarness, AgentHarnessTurnState, HarnessOpResult};

impl<S> AgentHarness<S>
where
    S: crate::session::types::SessionStorage + Clone + Send + Sync + 'static,
    S::Metadata: crate::session::types::HasSessionId + Send + Sync,
{
    pub(in crate::agent::harness) async fn create_turn_state(&self) -> HarnessOpResult<AgentHarnessTurnState> {
        let session = self.shared.session.lock().await;
        let context = session.build_context().await.map_err(session_error)?;
        let metadata = session.metadata().await;
        drop(session);

        let resources = self.shared.resources.lock().await.clone();
        let tools = self.shared.tools.lock().await.values().cloned().collect();
        let active_tools: Vec<AgentTool> = {
            let tools = self.shared.tools.lock().await;
            let names = self.shared.active_tool_names.lock().await;
            names.iter().filter_map(|name| tools.get(name).cloned()).collect()
        };
        let model = self.shared.model.lock().await.clone();
        let thinking_level = *self.shared.thinking_level.lock().await;
        let stream_options = {
            let guard = self.shared.stream_options.lock().await;
            clone_stream_options(&guard)
        };

        let system_prompt = match &*self.shared.system_prompt.lock().await {
            SystemPrompt::Static(text) => text.clone(),
            SystemPrompt::Dynamic(func) => {
                let session = self.shared.session.lock().await.clone();
                let ctx = crate::agent::harness::types::SystemPromptContext {
                    env: self.shared.env.clone(),
                    session,
                    model: model.clone(),
                    thinking_level,
                    active_tools: active_tools.clone(),
                    resources: resources.clone(),
                };
                func(ctx).await
            }
        };

        let mut system_prompt = system_prompt;
        if *self.shared.collaboration_mode.lock().await == CollaborationMode::Plan {
            system_prompt.push_str(plan_mode_system_prompt());
        }

        let base_tools: Vec<AgentTool> = active_tools
            .iter()
            .filter(|tool| !crate::collaboration::is_collaboration_tool(tool.name()))
            .cloned()
            .collect();
        self.shared
            .agent_control
            .lock()
            .await
            .refresh_config(system_prompt.clone(), model.clone(), base_tools)
            .await;

        Ok(AgentHarnessTurnState {
            messages: context.messages,
            resources,
            stream_options,
            session_id: metadata.session_id().to_string(),
            system_prompt,
            model,
            thinking_level,
            _tools: tools,
            active_tools,
        })
    }
}
