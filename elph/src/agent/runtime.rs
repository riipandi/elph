//! Factory for coding-agent sessions.

use anyhow::Result;
use elph_agent::create_goal_tools;
use elph_agent::{
    AgentGraphStore, AgentHarness, AgentHarnessOptions, AgentHarnessStreamOptions, BuiltinToolsBuilder, GoalRuntime,
    GoalStore, LocalExecutionEnv, McpToolRegistry, QueueMode, SubagentBootstrap, SystemPrompt,
};
use elph_core::utils::path::AppPaths;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;

use super::mcp_bootstrap::{discover_mcp_registry, start_mcp_notifications};
use super::model_registry::resolve_model;
use super::prompt::{agents_md_for_cwd, build_coding_system_prompt};
use super::resource_loader::{LoadResourcesResult, load_resources};
use super::session::{CodingAgentSession, CodingAgentSessionParams};
use super::session_manager::SessionManager;
use super::tool_policy::{agent_mode_from_setting, thinking_level_from_setting, to_agent_thinking};
use crate::platform::{Paths, Settings};
pub struct CreateSessionOptions<'a> {
    pub paths: &'a Paths,
    pub settings: &'a Settings,
    pub cwd: &'a Path,
    pub resume_id: Option<&'a str>,
    pub provider_override: Option<&'a str>,
    pub model_override: Option<&'a str>,
    /// When set, skips a second [`load_resources`] pass during session bootstrap.
    pub preloaded_resources: Option<LoadResourcesResult>,
    /// When true, MCP discovery is skipped; use [`super::mcp_bootstrap`] to load later.
    pub defer_mcp_load: bool,
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

    let resources = match options.preloaded_resources {
        Some(loaded) => loaded.resources,
        None => load_resources(options.paths, options.cwd, env.as_ref()).await.resources,
    };
    let mut tools = BuiltinToolsBuilder::all(env.clone()).build();
    tools.push(super::diagnostics::create_diagnostics_tool(&options.cwd.display().to_string()));

    // Create shared UI event channel for ask_user tool and session.
    let (ui_tx, ui_rx) = tokio::sync::mpsc::unbounded_channel();
    tools.push(super::ask_user::create_ask_user_tool(ui_tx.clone()));

    let (mcp_registry, mcp_config_warnings) = if options.defer_mcp_load {
        (Arc::new(McpToolRegistry::empty()), Vec::new())
    } else {
        let (registry, warnings) = discover_mcp_registry(options.paths).await;
        tools.extend(registry.create_agent_tools());
        (registry, warnings)
    };

    let goal_store = Arc::new(GoalStore::new(options.paths.metadata_db_path()));
    let goal_runtime = Arc::new(GoalRuntime::new(goal_store.clone(), session_id.clone()));
    tools.extend(create_goal_tools(goal_store, session_id.clone()));

    let thinking = to_agent_thinking(thinking_level_from_setting(&options.settings.session.thinking_level));
    let agent_graph = Arc::new(AgentGraphStore::new(options.paths.metadata_db_path()));
    // Map host settings → agnostic harness stream options (elph-agent never reads settings.json).
    let stream_options = AgentHarnessStreamOptions {
        timeout_ms: options.settings.provider_timeout_ms(),
        max_retries: Some(options.settings.provider.max_retries),
        ..AgentHarnessStreamOptions::default()
    };
    let subagent_bootstrap = SubagentBootstrap {
        project_key: session_manager.project_key().to_string(),
        cwd: options.cwd.display().to_string(),
        sessions_root: options.paths.sessions_dir().to_string_lossy().to_string(),
        resources: resources.clone(),
        stream_options: stream_options.clone(),
        thinking_level: thinking,
        agent_graph: Some(agent_graph),
    };

    let agent_mode = agent_mode_from_setting(&options.settings.session.agent_mode);
    let mode_state = Arc::new(Mutex::new(agent_mode));
    let cwd = options.cwd.to_path_buf();
    let agents_md = agents_md_for_cwd(options.cwd);
    let mode_for_prompt = Arc::clone(&mode_state);

    let system_prompt = SystemPrompt::Dynamic(Arc::new(move |ctx| {
        let cwd = cwd.clone();
        let agents_md = agents_md.clone();
        let mode_state = Arc::clone(&mode_for_prompt);
        Box::pin(async move {
            let mode = *mode_state.lock().await;
            let tool_names: Vec<String> = ctx.active_tools.iter().map(|t| t.name().to_string()).collect();
            build_coding_system_prompt(&cwd, &ctx.resources, &tool_names, agents_md.as_deref(), mode).unwrap_or_else(
                |error| {
                    log::warn!("coding system prompt render failed: {error}");
                    elph_agent::DEFAULT_SYSTEM_PROMPT.to_string()
                },
            )
        })
    }));

    let model = selection.model.clone();
    let models = Arc::clone(&selection.models);
    let harness = AgentHarness::new(AgentHarnessOptions {
        env,
        session,
        models,
        tools,
        resources,
        system_prompt,
        stream_options,
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

    let harness = Arc::new(harness);

    let session = CodingAgentSession::new(CodingAgentSessionParams {
        harness: harness.clone(),
        session_manager,
        session_id,
        selection,
        agent_mode,
        mode_state: Arc::clone(&mode_state),
        show_thinking: options.settings.ui.show_thinking,
        goal_runtime,
        mcp_registry: Some(Arc::clone(&mcp_registry)),
        ui_tx: ui_tx.clone(),
    })
    .await?;

    if !options.defer_mcp_load {
        start_mcp_notifications(&session, mcp_registry, mcp_config_warnings);
    }

    Ok((session, ui_rx))
}

pub async fn create_coding_session(options: CreateSessionOptions<'_>) -> Result<CodingAgentSession> {
    let (session, _rx) = create_coding_session_with_events(options).await?;
    Ok(session)
}
