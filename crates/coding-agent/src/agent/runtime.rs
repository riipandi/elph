//! Factory for coding-agent sessions.

use crate::utils::path::AppPaths;
use anyhow::Result;
use elph_agent::create_goal_tools_with_hook;
use elph_agent::{
    AgentGraphStore, AgentHarness, AgentHarnessOptions, AgentHarnessStreamOptions, BuiltinToolsBuilder, GoalRuntime,
    GoalStore, LocalExecutionEnv, QueueMode, RestoreOptions, SubagentBootstrap, SystemPrompt, TodoStore, TurnStore,
    create_todo_tools, is_mcp_tool,
};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;

use super::mcp_bootstrap::{discover_mcp_registry, start_mcp_notifications};
use super::model_registry::{resolve_model, selection_from_model};
use super::prompt::{CodingPromptOptions, agents_md_for_cwd, build_coding_system_prompt};
use super::resource_loader::{LoadResourcesResult, load_resources};
use super::session::{CodingAgentSession, CodingAgentSessionParams};
use super::session_manager::SessionManager;
use super::tool_policy::{thinking_level_from_setting, to_agent_thinking};
use crate::platform::{Paths, Settings};
use crate::types::AgentMode;
pub struct CreateSessionOptions<'a> {
    pub paths: &'a Paths,
    pub settings: &'a Settings,
    pub cwd: &'a Path,
    pub resume_id: Option<&'a str>,
    pub provider_override: Option<&'a str>,
    pub model_override: Option<&'a str>,
    /// Host override for agent mode (e.g. `elph run --brave`). Default: `build`.
    /// Not read from settings — mode is per-session.
    pub agent_mode: Option<crate::types::AgentMode>,
    /// When set, skips a second [`load_resources`] pass during session bootstrap.
    pub preloaded_resources: Option<LoadResourcesResult>,
    /// When true, MCP discovery is skipped; use [`super::mcp_bootstrap`] to load later.
    pub defer_mcp_load: bool,
    /// Whether the session runs in headless mode (`elph run`). Relaxes some tool
    /// defaults (e.g. no background-task timeout by default).
    pub headless: bool,
}

pub async fn create_coding_session_with_events(
    options: CreateSessionOptions<'_>,
) -> Result<(
    CodingAgentSession,
    tokio::sync::mpsc::UnboundedReceiver<super::events::AgentUiEvent>,
)> {
    // Open the shared store DB once and share the handle with every store so
    // they all connect from one open database instead of each opening the file.
    let database = Arc::new(crate::platform::datastore::ensure_database(options.paths).await?);

    // Best-effort session retention GC (settings-driven). Never mid-turn; skip if disabled.
    if options.settings.session.retention.enabled && options.settings.session.retention.gc_on_open {
        let r = &options.settings.session.retention;
        let policy = elph_agent::RetentionPolicy {
            enabled: true,
            max_sessions_per_cwd: r.max_sessions_per_cwd,
            max_session_age_days: r.max_session_age_days,
            max_store_db_bytes: r.max_store_db_bytes,
            protect_latest_per_cwd: r.protect_latest_per_cwd,
            protect_session_id: options.resume_id.map(|s| s.to_string()),
        };
        match elph_agent::run_full_session_gc(
            Arc::clone(&database),
            options.paths.memory_db_path(),
            Some(options.paths.data_dir().join("sessions")),
            policy,
            false,
        )
        .await
        {
            Ok(report) if !report.deleted_ids.is_empty() => {
                log::info!(
                    "session GC removed {} session(s) (examined {})",
                    report.deleted_ids.len(),
                    report.examined
                );
            }
            Ok(_) => {}
            Err(err) => log::warn!("session GC failed: {err:#}"),
        }
    }

    let env = Arc::new(LocalExecutionEnv::new(options.cwd));
    let session_manager = SessionManager::new_with_database(options.paths, options.cwd, database.clone())?;
    let session = session_manager.create(options.resume_id).await?;
    let session_id = {
        use elph_agent::session::types::HasSessionId;
        session.metadata().await.session_id().to_string()
    };

    // resolve_model and discover_mcp_registry are pure file reads independent
    // of each other and of the DB operations above — run them concurrently.
    let auth_store = options.paths.auth_store_path();

    // Open session-scoped MCP cache store (eager — creates the JSONL file now).
    let mcp_cache_path = session_manager.mcp_cache_path(&session_id);
    let mcp_cache = elph_agent::McpCacheStore::open(&mcp_cache_path, options.settings.mcp.cache_max_entries).ok();

    let ((selection, _overlay_stats), (mcp_registry, mcp_config_warnings)) = tokio::try_join!(
        resolve_model(
            options.settings,
            options.provider_override,
            options.model_override,
            Some(&auth_store),
        ),
        async {
            let (registry, warnings) = discover_mcp_registry(
                options.paths,
                mcp_cache.map(Arc::new),
                options.settings.mcp.cache_ttl_secs.saturating_mul(1000),
            )
            .await;
            Ok::<_, anyhow::Error>((registry, warnings))
        },
    )?;

    let resources = match options.preloaded_resources {
        Some(loaded) => loaded.resources,
        None => load_resources(options.paths, options.cwd, env.as_ref()).await.resources,
    };
    // Provide the loaded skill set so `list_skills` (on-demand catalog) can be
    // registered alongside the built-in tools.
    let mut tools = BuiltinToolsBuilder::all(env.clone())
        .with_skills(resources.skills.clone())
        .build();

    // Shared memory runtime (tools + hooks + bootstrap use one store / task id).
    let memory_opts = crate::memory::runtime::MemoryRuntimeOptions::from_settings(&options.settings.memory);
    let memory_runtime = Arc::new(crate::memory::MemoryRuntime::with_options_and_db(
        options.paths.clone(),
        session_id.clone(),
        memory_opts,
        Some(database.clone()),
    ));
    tools.extend(crate::memory::tools::create_memory_tools(Arc::clone(&memory_runtime)));
    if options.settings.codegraph.enabled {
        tools.extend(crate::codegraph::tools::create_codegraph_tools_with_db(
            options.paths.clone(),
            Some(database.clone()),
        ));
    }

    // Create shared UI event channel for ask_user tool and session.
    let (ui_tx, ui_rx) = tokio::sync::mpsc::unbounded_channel();
    tools.push(super::ask_user::create_ask_user_tool(ui_tx.clone()));
    tools.push(super::mode_change::create_mode_change_tool(ui_tx.clone()));

    if !options.defer_mcp_load {
        tools.extend(mcp_registry.create_agent_tools().await);
    }

    let goal_store = Arc::new(GoalStore::new(options.paths.memory_db_path()).with_database(database.clone()));
    let goal_runtime = Arc::new(GoalRuntime::new(goal_store.clone(), session_id.clone()));
    // Goals bridge: terminal goal status → work memory for future recall.
    let memory_for_goals = Arc::clone(&memory_runtime);
    let goal_hook: Option<elph_agent::GoalStatusHook> = Some(Arc::new(move |goal| {
        let runtime = Arc::clone(&memory_for_goals);
        Box::pin(async move {
            let status = goal.status.as_str();
            if let Err(err) = runtime.record_goal_outcome(&goal.id, &goal.objective, status).await {
                log::warn!("memory goals bridge: {err:#}");
            } else {
                log::debug!("memory.write kind=work source=goal id={} status={status}", goal.id);
            }
        })
    }));
    tools.extend(create_goal_tools_with_hook(goal_store, session_id.clone(), goal_hook));

    let todo_store = Arc::new(TodoStore::new(options.paths.memory_db_path()).with_database(database.clone()));
    tools.extend(create_todo_tools(todo_store, session_id.clone()));

    // Clamp default thinking (new-session seed) to the resolved model catalog.
    let thinking = {
        let raw = thinking_level_from_setting(&options.settings.models.default_thinking_level);
        let clamped = raw.clamp_for_model(&selection.model);
        to_agent_thinking(clamped)
    };
    let agent_graph = Arc::new(AgentGraphStore::new(options.paths.memory_db_path()).with_database(database.clone()));
    // Map host settings → agnostic harness stream options (elph-agent never reads settings.json).
    let stream_options = AgentHarnessStreamOptions {
        timeout_ms: options.settings.provider_timeout_ms(),
        max_retries: Some(options.settings.max_retries),
        ..AgentHarnessStreamOptions::default()
    };
    let subagent_bootstrap = SubagentBootstrap {
        cwd: options.cwd.display().to_string(),
        store_db_path: options.paths.memory_db_path().to_string_lossy().to_string(),
        resources: resources.clone(),
        stream_options: stream_options.clone(),
        thinking_level: thinking,
        prompt_encoding: options.settings.prompt_encoding.clone(),
        agent_graph: Some(agent_graph),
        database: Some(database.clone()),
        // Persistent per-subagent artifacts: APP_DATA/sessions/<SESSION_ID>/subagents/<agent_id>/
        outputs_root: Some(session_manager.artifact_dir_for(&session_id)),
    };

    // Agent mode is per-session; default build unless the host overrides (e.g. --brave).
    let agent_mode = options.agent_mode.unwrap_or(AgentMode::Build);
    let mode_state = Arc::new(Mutex::new(agent_mode));
    let cwd = options.cwd.to_path_buf();
    let agents_md = agents_md_for_cwd(options.cwd);
    let mode_for_prompt = Arc::clone(&mode_state);

    // Build memory context from top-weighted memories for the system prompt.
    // Lock errors are handled internally (logged + empty context returned).
    let ctx = crate::memory::hooks::build_memories_context(memory_runtime.as_ref())
        .await
        .unwrap_or_default();
    let injected_memory = if ctx.is_empty() { None } else { Some(ctx) };
    // Static (per-run) prompt knobs; `mode` is filled in per turn from `mode_state`.
    let prompt_options = CodingPromptOptions {
        mode: agent_mode,
        preferred_chat_language: options.settings.preferred_chat_language.clone(),
        codegraph_enabled: options.settings.codegraph.enabled,
        ste_enabled: options.settings.simplified_technical_english,
    };

    let system_prompt = SystemPrompt::Dynamic(Arc::new(move |ctx| {
        let cwd = cwd.clone();
        let agents_md = agents_md.clone();
        let mode_state = Arc::clone(&mode_for_prompt);
        let memory_section = injected_memory.clone();
        let mut prompt_options = prompt_options.clone();
        Box::pin(async move {
            prompt_options.mode = *mode_state.lock().await;
            let tool_names: Vec<String> = ctx.active_tools.iter().map(|t| t.name().to_string()).collect();
            let mut prompt =
                build_coding_system_prompt(&cwd, &ctx.resources, &tool_names, agents_md.as_deref(), &prompt_options)
                    .unwrap_or_else(|error| {
                        log::warn!("coding system prompt render failed: {error}");
                        elph_agent::DEFAULT_SYSTEM_PROMPT.to_string()
                    });

            // Append memory context section at the end of the system prompt.
            if let Some(ref mem) = memory_section {
                prompt.push_str("\n\n");
                prompt.push_str(mem);
            }

            prompt
        })
    }));

    let model = selection.model.clone();
    let models = Arc::clone(&selection.models);
    let compaction_settings = options.settings.compaction.to_agent_settings();
    // MCP tools are registered for execution/discovery but default-inactive: omit them
    // from the initial active set so empty/`None` restore does not activate every
    // connected MCP server's schemas. Session-tree `ActiveToolsChange` (lazy activation
    // or resume mid-session) still restores previously activated MCP names.
    let active_tool_names: Vec<String> = tools
        .iter()
        .map(|t| t.name().to_string())
        .filter(|name| !is_mcp_tool(name))
        .collect();
    // Prefer restore for semi-durable recovery (queues, ops, tool-result repair, config rehydrate).
    let harness = AgentHarness::restore(
        AgentHarnessOptions {
            env,
            session,
            models,
            tools,
            resources,
            system_prompt,
            stream_options,
            model,
            thinking_level: thinking,
            active_tool_names,
            steering_mode: QueueMode::OneAtATime,
            follow_up_mode: QueueMode::OneAtATime,
            compaction_settings,
            goal_runtime: Some(goal_runtime.clone()),
            turn_store: Some(Arc::new(
                TurnStore::new(options.paths.memory_db_path()).with_database(database.clone()),
            )),
            subagent_bootstrap: Some(subagent_bootstrap),
            shared_registry: None,
            agent_control: None,
            headless: options.headless,
            terminals_dir: Some(session_manager.artifact_dir_for(&session_id).join("terminals")),
        },
        RestoreOptions::default(),
    )
    .await
    .map_err(|e| anyhow::anyhow!("{e}"))?;
    // Host settings → harness prompt encoding (None keeps the env fallback).
    harness.set_prompt_encoding(options.settings.prompt_encoding.clone());

    // Wire automatic memory hooks (per-turn recall, auto-correction, work capture, task lifecycle).
    // Runs best-effort: errors are logged and don't prevent session startup.
    if let Err(err) = crate::memory::hooks::register_automatic_memory_hooks(&harness, Arc::clone(&memory_runtime)).await
    {
        log::warn!("automatic memory hooks: {err:#}");
    }

    let harness = Arc::new(harness);
    let restored_selection = {
        let restored_model = harness.get_model().await;
        selection_from_model(&restored_model, Arc::clone(&selection.models))
    };

    let session = CodingAgentSession::new(CodingAgentSessionParams {
        harness: harness.clone(),
        session_manager,
        session_id,
        selection: restored_selection,
        agent_mode,
        mode_state: Arc::clone(&mode_state),
        show_thinking: options.settings.ui.show_thinking,
        goal_runtime,
        mcp_registry: Some(Arc::clone(&mcp_registry)),
        ui_tx: ui_tx.clone(),
        title_model: options.settings.models.session_title_model.clone(),
        preferred_chat_language: options.settings.preferred_chat_language.clone(),
        compaction_model_ref: options.settings.models.compaction_model.clone(),
        codegraph_enabled: options.settings.codegraph.enabled,
        ste_enabled: options.settings.simplified_technical_english,
    })
    .await?;

    start_mcp_notifications(&session, Arc::clone(&mcp_registry), mcp_config_warnings);

    Ok((session, ui_rx))
}

pub async fn create_coding_session(options: CreateSessionOptions<'_>) -> Result<CodingAgentSession> {
    let (session, _rx) = create_coding_session_with_events(options).await?;
    Ok(session)
}
