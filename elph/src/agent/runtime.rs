//! Factory for coding-agent sessions.

use anyhow::Result;
use elph_agent::{
    AgentGraphStore, AgentHarness, AgentHarnessOptions, AgentHarnessStreamOptions, GoalRuntime, GoalStore,
    LocalExecutionEnv, McpToolRegistry, QueueMode, SubagentBootstrap, SystemPrompt, create_all_tools,
    create_goal_tools,
};
use elph_core::utils::path::AppPaths;
use std::path::Path;
use std::sync::Arc;
use tracing::warn;

use super::model_registry::resolve_model;
use super::resource_loader::load_resources;
use super::session::CodingAgentSession;
use super::session_manager::SessionManager;
use super::system_prompt::{agents_md_for_cwd, build_system_prompt};
use super::tool_policy::{agent_mode_from_setting, thinking_level_from_setting, to_agent_thinking};
use crate::platform::{Paths, Settings};

pub struct CreateSessionOptions<'a> {
    pub paths: &'a Paths,
    pub settings: &'a Settings,
    pub cwd: &'a Path,
    pub resume_id: Option<&'a str>,
    pub provider_override: Option<&'a str>,
    pub model_override: Option<&'a str>,
}

pub async fn create_coding_session_with_events(
    options: CreateSessionOptions<'_>,
) -> Result<(
    CodingAgentSession,
    tokio::sync::mpsc::UnboundedReceiver<super::events::AgentUiEvent>,
)> {
    crate::platform::ensure_datastore(options.paths).await?;

    let env = Arc::new(LocalExecutionEnv::new(options.cwd));
    let session_manager = SessionManager::new(options.paths, env.clone(), options.cwd)?;
    let session = session_manager.create(options.resume_id).await?;
    let session_id = {
        use elph_agent::session::types::HasSessionId;
        session.metadata().await.session_id().to_string()
    };
    let selection = resolve_model(options.settings, options.provider_override, options.model_override).await?;

    let resources = load_resources(options.paths, options.cwd);
    let mut tools = create_all_tools(env.clone());

    let mcp_config = crate::platform::mcp::load_config(options.paths)?;
    let mcp_registry = match McpToolRegistry::load(mcp_config).await {
        Ok(registry) => Arc::new(registry),
        Err(error) => {
            warn!("MCP tool discovery failed: {error}");
            Arc::new(McpToolRegistry::empty())
        }
    };
    tools.extend(mcp_registry.create_agent_tools());

    let goal_store = Arc::new(GoalStore::new(options.paths.metadata_db_path()));
    let goal_runtime = Arc::new(GoalRuntime::new(goal_store.clone(), session_id.clone()));
    tools.extend(create_goal_tools(goal_store, session_id.clone()));

    let thinking = to_agent_thinking(thinking_level_from_setting(&options.settings.session.thinking_level));
    let agent_graph = Arc::new(AgentGraphStore::new(options.paths.metadata_db_path()));
    let subagent_bootstrap = SubagentBootstrap {
        project_key: session_manager.project_key().to_string(),
        cwd: options.cwd.display().to_string(),
        sessions_root: options.paths.sessions_dir().to_string_lossy().to_string(),
        resources: resources.clone(),
        stream_options: AgentHarnessStreamOptions::default(),
        thinking_level: thinking,
        agent_graph: Some(agent_graph),
    };
    let tool_names: Vec<String> = tools.iter().map(|t| t.name().to_string()).collect();
    let agents_md = agents_md_for_cwd(options.cwd);
    let system_prompt = build_system_prompt(options.cwd, &resources, &tool_names, agents_md.as_deref());
    let agent_mode = agent_mode_from_setting(&options.settings.session.agent_mode);

    let model = selection.model.clone();
    let models = Arc::clone(&selection.models);
    let harness = AgentHarness::new(AgentHarnessOptions {
        env,
        session,
        models,
        tools,
        resources,
        system_prompt: SystemPrompt::Static(system_prompt),
        stream_options: AgentHarnessStreamOptions::default(),
        model,
        thinking_level: thinking,
        active_tool_names: vec![],
        steering_mode: QueueMode::OneAtATime,
        follow_up_mode: QueueMode::OneAtATime,
        goal_runtime: Some(goal_runtime.clone()),
        subagent_bootstrap: Some(subagent_bootstrap),
        shared_registry: None,
        agent_control: None,
    })
    .map_err(|e| anyhow::anyhow!("{e}"))?;

    CodingAgentSession::new(
        Arc::new(harness),
        session_manager,
        session_id,
        selection,
        agent_mode,
        options.settings.show_thinking,
        goal_runtime,
    )
    .await
}

pub async fn create_coding_session(options: CreateSessionOptions<'_>) -> Result<CodingAgentSession> {
    let (session, _rx) = create_coding_session_with_events(options).await?;
    Ok(session)
}
