//! Factory for coding-agent sessions.

use anyhow::Result;
use elph_agent::{
    AgentGraphStore, AgentHarness, AgentHarnessOptions, AgentHarnessStreamOptions, BuiltinToolsBuilder, GoalRuntime,
    GoalStore, LocalExecutionEnv, McpLoadOptions, McpToolRegistry, QueueMode, SubagentBootstrap, SystemPrompt,
    create_goal_tools,
};
use elph_core::utils::path::AppPaths;
use std::path::Path;
use std::sync::Arc;

use super::model_registry::resolve_model;
use super::resource_loader::load_resources;
use super::session::{CodingAgentSession, CodingAgentSessionParams};
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
    let mut tools = BuiltinToolsBuilder::all(env.clone()).build();
    tools.push(super::diagnostics::create_diagnostics_tool(&options.cwd.display().to_string()));

    // Create shared UI event channel for ask_user tool and session.
    let (ui_tx, ui_rx) = tokio::sync::mpsc::unbounded_channel();
    tools.push(super::ask_user::create_ask_user_tool(ui_tx.clone()));

    let mcp_config = crate::platform::mcp::load_config(options.paths)?;
    let auth_store_path = options.paths.auth_store_path();
    let load_options = McpLoadOptions {
        auth_store_path: Some(auth_store_path),
        ..McpLoadOptions::default()
    };
    let mcp_registry = match McpToolRegistry::load_with_options(mcp_config, load_options).await {
        Ok(registry) => {
            let report = registry.load_report();
            if report.servers_failed > 0 {
                log::warn!(
                    "MCP discovery finished with server failures: ok={} failed={} tools={}",
                    report.servers_ok,
                    report.servers_failed,
                    report.tools_loaded
                );
                for server in &report.servers {
                    if !server.ok {
                        log::warn!("MCP server unavailable: server={} error={}", server.name, server.message);
                    }
                }
            }
            Arc::new(registry)
        }
        Err(error) => {
            log::warn!("MCP tool discovery failed: {error}");
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

    let harness = Arc::new(harness);

    let session = CodingAgentSession::new(CodingAgentSessionParams {
        harness: harness.clone(),
        session_manager,
        session_id,
        selection,
        agent_mode,
        show_thinking: options.settings.show_thinking,
        goal_runtime,
        mcp_registry: Some(Arc::clone(&mcp_registry)),
        ui_tx,
    })
    .await?;

    // Catalog hot-reload + progress → TUI status (after ui channel exists).
    {
        let harness_for_reload = Arc::clone(&session.harness());
        let ui_tx = session.ui_event_sender();
        let started = mcp_registry.spawn_event_loop(
            move |registry| {
                let harness = Arc::clone(&harness_for_reload);
                tokio::spawn(async move {
                    if let Err(error) = apply_mcp_tools_to_harness(&harness, &registry).await {
                        log::warn!("failed to apply MCP hot-reload tools: {error}");
                    } else {
                        log::info!("MCP tools hot-reloaded into agent harness");
                    }
                });
            },
            move |status| {
                let _ = ui_tx.send(super::events::AgentUiEvent::Status(status));
            },
        );
        if started {
            log::info!("MCP event loop (list_changed + progress) started");
        }
    }

    Ok((session, ui_rx))
}

async fn apply_mcp_tools_to_harness(
    harness: &AgentHarness<elph_agent::SessionDirStorage>,
    registry: &Arc<McpToolRegistry>,
) -> Result<()> {
    let mcp_tools = registry.create_agent_tools();
    let mcp_names: Vec<String> = mcp_tools.iter().map(|t| t.name().to_string()).collect();
    let mut kept: Vec<_> = harness
        .get_tools()
        .await
        .into_iter()
        .filter(|t| !t.name().starts_with("mcp_"))
        .collect();
    kept.extend(mcp_tools);

    let prev_active: Vec<String> = harness
        .get_active_tools()
        .await
        .into_iter()
        .map(|t| t.name().to_string())
        .collect();
    // Empty active list means "all tools active" — preserve that convention.
    let active_opt = if prev_active.is_empty() {
        None
    } else {
        let mut next: Vec<String> = prev_active.into_iter().filter(|n| !n.starts_with("mcp_")).collect();
        next.extend(mcp_names);
        Some(next)
    };

    harness
        .set_tools(kept, active_opt)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    Ok(())
}

pub async fn create_coding_session(options: CreateSessionOptions<'_>) -> Result<CodingAgentSession> {
    let (session, _rx) = create_coding_session_with_events(options).await?;
    Ok(session)
}
