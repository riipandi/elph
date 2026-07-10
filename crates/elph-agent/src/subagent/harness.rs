//! Session-backed subagent harness.

use std::sync::Arc;

use elph_ai::Model;

use super::control::AgentControl;
use super::registry::AgentRegistry;
use super::types::{SubagentBootstrap, SubagentInfo, SubagentLimits};
use crate::env::LocalExecutionEnv;
use crate::harness::{AgentHarness, AgentHarnessError, AgentHarnessOptions, SystemPrompt};
use crate::session::{SessionDirRepo, SessionDirRepoCreateOptions, SessionDirStorage};
use crate::types::{AgentTool, QueueMode};

pub struct SubagentHarness {
    harness: Arc<AgentHarness<SessionDirStorage>>,
    info: SubagentInfo,
}

impl SubagentHarness {
    pub fn info(&self) -> &SubagentInfo {
        &self.info
    }

    pub fn harness(&self) -> &AgentHarness<SessionDirStorage> {
        &self.harness
    }

    pub async fn followup(&self, message: String) -> Result<(), String> {
        self.harness.prompt(message, None).await.map_err(|e| e.to_string())?;
        self.harness.wait_for_idle().await.map_err(|e| e.to_string())?;
        Ok(())
    }

    pub async fn wait_idle(&self) -> Result<(), String> {
        self.harness.wait_for_idle().await.map_err(|e| e.to_string())
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn spawn_subagent_harness(
    bootstrap: &SubagentBootstrap,
    env: Arc<LocalExecutionEnv>,
    model: Model,
    models: Arc<elph_ai::Models>,
    _stream_fn: crate::types::StreamFn,
    base_tools: Vec<AgentTool>,
    root_session_id: &str,
    agent_id: &str,
    task_name: &str,
    agent_path: &str,
    depth: u32,
    limits: SubagentLimits,
    shared_registry: Arc<AgentRegistry>,
    agent_control: Arc<AgentControl>,
    system_prompt: String,
) -> Result<Arc<SubagentHarness>, String> {
    let repo = SessionDirRepo::new(
        env.clone(),
        bootstrap.sessions_root.clone(),
        bootstrap.project_key.clone(),
    );
    let child_session_id = crate::session::id::generate_session_id();
    let session = repo
        .create(SessionDirRepoCreateOptions {
            cwd: bootstrap.cwd.clone(),
            project_key: bootstrap.project_key.clone(),
            id: Some(child_session_id.clone()),
            parent_session_id: Some(root_session_id.to_string()),
            system_prompt: Some(system_prompt.clone()),
        })
        .await
        .map_err(|e| e.to_string())?;

    if let Some(graph) = &bootstrap.agent_graph {
        graph
            .record_spawn(root_session_id, &child_session_id, agent_path, depth)
            .await
            .map_err(|e| e.to_string())?;
    }

    let mut tools = base_tools;
    if depth < limits.max_depth {
        tools.extend(crate::tools::create_multi_agent_tools(agent_control.clone()));
    }

    let harness = AgentHarness::new(AgentHarnessOptions {
        env,
        session,
        models,
        tools,
        resources: bootstrap.resources.clone(),
        system_prompt: SystemPrompt::Static(system_prompt),
        stream_options: bootstrap.stream_options.clone(),
        model,
        thinking_level: bootstrap.thinking_level,
        active_tool_names: vec![],
        steering_mode: QueueMode::OneAtATime,
        follow_up_mode: QueueMode::OneAtATime,
        goal_runtime: None,
        subagent_bootstrap: Some(bootstrap.clone()),
        shared_registry: Some(shared_registry),
        agent_control: Some(agent_control),
    })
    .map_err(|e: AgentHarnessError| e.to_string())?;

    let info = SubagentInfo {
        id: agent_id.to_string(),
        session_id: child_session_id,
        task_name: task_name.to_string(),
        agent_path: agent_path.to_string(),
        depth,
        status: super::types::SubagentStatus::Pending,
        parent_session_id: root_session_id.to_string(),
    };

    Ok(Arc::new(SubagentHarness {
        harness: Arc::new(harness),
        info,
    }))
}
