//! Factory for coding-agent sessions.

use crate::utils::path::AppPaths;
use anyhow::Result;
use elph_agent::BuiltinToolsBuilder;
use elph_agent::QueueMode;
use elph_agent::agent::subagent::{AgentGraphStore, SubagentBootstrap};
use elph_agent::collaboration::is_mcp_tool;
use elph_agent::goals::create_goal_tools_with_hook;
use elph_agent::goals::{GoalRuntime, GoalStore};
use elph_agent::harness::{AgentHarness, AgentHarnessOptions, AgentHarnessStreamOptions, RestoreOptions, SystemPrompt};
use elph_agent::runtime::LocalExecutionEnv;
use elph_agent::session_summary::{SessionSummaryStore, create_session_summary_tool};
use elph_agent::todos::{TodoHook, TodoStore, WorkTracker, create_todo_tools_with_hook};
use elph_agent::turns::TurnStore;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::Mutex;

use super::mcp_bootstrap::{discover_mcp_registry, start_mcp_notifications};
use super::model_registry::{resolve_model, selection_from_model};
use super::prompt::{CodingPromptOptions, agents_md_for_cwd, build_coding_system_prompt};
use super::resource_loader::{LoadResourcesResult, load_resources};
use super::session::{CodingAgentSession, CodingAgentSessionParams};
use super::session_manager::SessionManager;
use super::tool_policy::{thinking_level_from_setting, to_agent_thinking};
use super::worker_runtime::{WorkerRuntime, WorkerRuntimeStart};
use crate::platform::{Paths, Settings};
use crate::types::AgentMode;
pub struct CreateSessionOptions<'a> {
    pub paths: &'a Paths,
    pub settings: &'a Settings,
    pub cwd: &'a Path,
    pub resume_id: Option<&'a str>,
    /// When true with `resume_id`, create a new session with that id if missing.
    pub create_if_missing: bool,
    /// Optional display name applied on new session create.
    pub session_name: Option<&'a str>,
    pub provider_override: Option<&'a str>,
    pub model_override: Option<&'a str>,
    /// Thinking level seed override for a **new** session (e.g. last used level in
    /// this project). `None` → `settings.models.defaultThinkingLevel`. Ignored on
    /// resume: the harness restores the thinking level from the session tree.
    pub thinking_override: Option<&'a str>,
    /// Host override for agent mode (e.g. `elph run --mode`). Default: `build` (TUI)
    /// or headless caller default (brave for `elph run`).
    /// Not read from settings — mode is per-session.
    pub agent_mode: Option<crate::types::AgentMode>,
    /// Full system prompt override (replaces compiled coding prompt for this run).
    pub system_prompt_override: Option<&'a str>,
    /// When set, skips a second [`load_resources`] pass during session bootstrap.
    pub preloaded_resources: Option<LoadResourcesResult>,
    /// When true, MCP discovery is skipped; use [`super::mcp_bootstrap`] to load later.
    pub defer_mcp_load: bool,
    /// When true, session retention GC runs in the background instead of blocking
    /// session creation (TUI fast path — `AgentReady` must not wait for GC).
    pub defer_session_gc: bool,
    /// When true, the memory store warm-up (embedder/store init) is skipped during
    /// session creation; the static bootstrap hint is still injected. The first
    /// turn's recall warms the store anyway.
    pub defer_memory_warm: bool,
    /// Whether the session runs in headless mode (`elph run`). Relaxes some tool
    /// defaults (e.g. no background-task timeout by default).
    pub headless: bool,
    /// WASM extension host; bound to the harness after restore when present.
    pub extension_host: Option<&'a crate::extensions::ExtensionHost>,
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

    let mut env = LocalExecutionEnv::new(options.cwd);
    if let Some(path) = options
        .settings
        .shell_path
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        let expanded = if let Some(rest) = path.strip_prefix("~/") {
            std::env::var_os("HOME")
                .map(|h| std::path::PathBuf::from(h).join(rest))
                .unwrap_or_else(|| std::path::PathBuf::from(path))
        } else {
            std::path::PathBuf::from(path)
        };
        env = env.with_shell_path(expanded);
    }
    if let Some(prefix) = options
        .settings
        .shell_command_prefix
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        env = env.with_command_prefix(prefix);
    }
    let env = Arc::new(env);
    let workers_cfg = &options.settings.workers;
    // One worker_id for lease + registry + file claims for this process.
    let worker_id = WorkerRuntime::new_worker_id();
    let lease_stale_secs = workers_cfg.lease_stale_secs.max(1);
    let mut session_manager = SessionManager::new_with_database(options.paths, options.cwd, database.clone())?;
    if workers_cfg.enabled {
        session_manager = session_manager.with_session_lease(worker_id.clone(), lease_stale_secs);
    }
    let session = session_manager
        .create_with_options(options.resume_id, options.create_if_missing, options.session_name)
        .await?;
    let session_id = {
        use elph_agent::session::types::HasSessionId;
        session.metadata().await.session_id().to_string()
    };
    let project_key = session_manager.project_key().to_string();

    // Best-effort session retention GC (settings-driven). Never mid-turn; skip if disabled.
    // Runs after the session row exists so the new/resumed session is always protected
    // (`protect_session_id`) — a detached GC can never delete the session being opened.
    if options.settings.session.enabled && options.settings.session.gc_on_open {
        let r = &options.settings.session;
        let policy = elph_agent::session::RetentionPolicy {
            enabled: true,
            max_sessions_per_cwd: r.max_sessions_per_cwd,
            max_session_age_days: r.max_session_age_days,
            max_store_db_bytes: r.max_store_db_bytes,
            protect_latest_per_cwd: r.protect_latest_per_cwd,
            protect_session_id: Some(session_id.clone()),
        };
        let database_for_gc = Arc::clone(&database);
        let db_path = options.paths.memory_db_path();
        let sessions_root = options.paths.data_dir().join("sessions");
        let run_gc = async move {
            match elph_agent::session::run_full_session_gc(database_for_gc, db_path, Some(sessions_root), policy, false)
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
        };
        if options.defer_session_gc {
            // TUI fast path: `AgentReady` must not wait for retention GC. Run it
            // detached; it shares the open DB handle and the current session is
            // protected by the policy, so it never deletes the session being opened.
            tokio::spawn(run_gc);
        } else {
            run_gc.await;
        }
    }

    // resolve_model and discover_mcp_registry are pure file reads independent
    // of each other and of the DB operations above — run them concurrently.
    let auth_store = options.paths.auth_store_path();

    // Open session-scoped MCP cache store (eager — creates the JSONL file now).
    let (selection, _overlay_stats) = resolve_model(
        options.settings,
        options.provider_override,
        options.model_override,
        Some(&auth_store),
    )
    .await?;
    let mcp_cache_path = session_manager.mcp_cache_path(&session_id);
    let mcp_cfg = crate::platform::mcp::load_config(options.paths).unwrap_or_default();
    let mcp_cache = elph_agent::mcp::McpCacheStore::open(&mcp_cache_path, mcp_cfg.cache_max_entries_or_default())
        .ok()
        .map(Arc::new);
    let default_cache_ttl_ms = mcp_cfg.cache_ttl_secs_or_default().saturating_mul(1000);
    let (mcp_registry, mcp_config_warnings) = if options.defer_mcp_load {
        let (mcp_config, warnings) = crate::platform::mcp::load_config_best_effort(options.paths);
        for warning in &warnings {
            log::warn!("{warning}");
        }
        let load_options = elph_agent::mcp::McpLoadOptions {
            auth_store_path: Some(options.paths.auth_store_path()),
            cache_store: mcp_cache.clone(),
            default_cache_ttl_ms,
            skip_startup_discovery: true,
            ..elph_agent::mcp::McpLoadOptions::default()
        };
        let registry = match elph_agent::mcp::McpToolRegistry::load_with_options(mcp_config, load_options).await {
            Ok(registry) => Arc::new(registry),
            Err(error) => {
                log::warn!("MCP deferred registry load failed: {error}");
                Arc::new(elph_agent::mcp::McpToolRegistry::empty())
            }
        };
        (registry, warnings)
    } else {
        discover_mcp_registry(options.paths, mcp_cache, default_cache_ttl_ms).await
    };

    let resources = match options.preloaded_resources {
        Some(loaded) => loaded.resources,
        None => {
            load_resources(options.paths, options.cwd, env.as_ref(), options.settings)
                .await
                .resources
        }
    };

    // Multi-worker: start before built-in tools so path claims + worker_* tools wire in.
    let worker_runtime = if workers_cfg.enabled {
        let desired_name = workers_cfg
            .name
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .unwrap_or_else(default_worker_name);
        match WorkerRuntime::start(WorkerRuntimeStart {
            database: database.clone(),
            db_path: options.paths.memory_db_path(),
            worker_id: worker_id.clone(),
            session_id: session_id.clone(),
            project_key: project_key.clone(),
            desired_name,
            purpose: workers_cfg.purpose.clone(),
            model: Some(format!("{}/{}", selection.model.provider, selection.model_id)),
            heartbeat_secs: workers_cfg.heartbeat_secs.max(1),
            stale_secs: lease_stale_secs,
            ask_timeout_ms: workers_cfg.ask_timeout_ms,
            max_hops: workers_cfg.max_hops,
            tui_show_peers: workers_cfg.tui_show_peers,
            file_leases: workers_cfg.file_leases,
            inbox_poll_ms: workers_cfg.inbox_poll_ms,
        })
        .await
        {
            Ok(rt) => {
                log::info!("worker registered name={} id={} session={}", rt.name, rt.worker_id, session_id);
                Some(rt)
            }
            Err(err) => {
                log::warn!("worker registry start failed (lease still held): {err:#}");
                None
            }
        }
    } else {
        None
    };

    let path_claims = worker_runtime.as_ref().and_then(|rt| {
        if !rt.file_leases_enabled() {
            return None;
        }
        Some(std::sync::Arc::new(elph_agent::workers::PathClaimContext::new(
            rt.file_leases(),
            rt.project_key.clone(),
            rt.worker_id.clone(),
            rt.session_id.clone(),
            rt.stale_secs(),
        )))
    });

    // Provide the loaded skill set so `list_skills` (on-demand catalog) can be
    // registered alongside the built-in tools.
    let mut tools = BuiltinToolsBuilder::all(env.clone())
        .with_skills(resources.skills.clone())
        .with_path_claims(path_claims)
        .build();
    if let Some(rt) = worker_runtime.as_ref() {
        tools.extend(rt.create_tools());
    }

    // Shared memory runtime (tools + hooks + bootstrap use one store / task id).
    let memory_opts = crate::memory::runtime::MemoryRuntimeOptions::from_settings(&options.settings.memory);
    let memory_runtime = Arc::new(crate::memory::MemoryRuntime::with_options_and_db(
        options.paths.clone(),
        session_id.clone(),
        memory_opts,
        Some(database.clone()),
    ));
    tools.extend(crate::memory::tools::create_memory_tools(Arc::clone(&memory_runtime)));

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
    let goal_hook: Option<elph_agent::goals::GoalStatusHook> = Some(Arc::new(move |goal| {
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
    let goal_store_for_prompt = Arc::clone(&goal_store);
    tools.extend(create_goal_tools_with_hook(goal_store, session_id.clone(), goal_hook));

    let todo_store = Arc::new(TodoStore::new(options.paths.memory_db_path()).with_database(database.clone()));
    let ui_tx_for_todo = ui_tx.clone();
    let todo_hook: TodoHook = Arc::new(move |items| {
        let ui_tx = ui_tx_for_todo.clone();
        Box::pin(async move {
            log::debug!("todo_hook: sending TodoUpdated event with {} items", items.len());
            if let Err(err) = ui_tx.send(crate::agent::AgentUiEvent::TodoUpdated { items }) {
                log::warn!("todo_hook: failed to send event: {err}");
            }
        })
    });
    // Work tracker: enforces honest progress by requiring actual mutating
    // tool calls between marking an item in_progress and marking it completed.
    let work_tracker = Arc::new(WorkTracker::new());
    let work_tracker_for_tools = work_tracker.clone();
    // Keep a handle for continuity brief + TUI rehydrate (tools take another Arc clone).
    let todo_store_for_prompt = Arc::clone(&todo_store);
    tools.extend(create_todo_tools_with_hook(
        Arc::clone(&todo_store),
        session_id.clone(),
        Some(todo_hook.clone()),
        Some(work_tracker_for_tools),
    ));

    // Session summary store: one row per session, upserted on compaction.
    // Read on demand via the `get_session_summary` agent tool.
    let summary_store =
        Arc::new(SessionSummaryStore::new(options.paths.memory_db_path()).with_database(database.clone()));
    tools.push(create_session_summary_tool(Arc::clone(&summary_store)));

    // Clamp default thinking (new-session seed) to the resolved model catalog.
    // `thinking_override` (last used level from the latest session) wins over
    // `settings.models.defaultThinkingLevel`. Ignored on resume — the harness
    // restores the session's own thinking level from its tree.
    let thinking = {
        let raw = options
            .thinking_override
            .map(thinking_level_from_setting)
            .unwrap_or_else(|| thinking_level_from_setting(&options.settings.models.default_thinking_level));
        let clamped = raw.clamp_for_model(&selection.model);
        to_agent_thinking(clamped)
    };
    let agent_graph = Arc::new(AgentGraphStore::new(options.paths.memory_db_path()).with_database(database.clone()));
    // Map host settings → agnostic harness stream options (elph-agent never reads settings.json).
    let stream_options = AgentHarnessStreamOptions {
        timeout_ms: options.settings.provider_timeout_ms(),
        max_retries: Some(options.settings.max_retries),
        thinking_budgets: options.settings.models.thinking_budgets.clone(),
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
    let plan_reentry = Arc::new(AtomicBool::new(false));
    let plan_reentry_for_prompt = Arc::clone(&plan_reentry);

    // Build memory context from top-weighted memories for the system prompt.
    // Lock errors are handled internally (logged + empty context returned).
    // The bootstrap context is a static turn-only hint (no real recall). The store
    // warm-up is deferred for the TUI fast path — `AgentReady` must not wait for
    // the embedder/store init. A detached warm-up task keeps the store ready before
    // the first turn (which would otherwise block on `ensure_store`), so recall on
    // turn one is never delayed by a cold embedder.
    let ctx = crate::memory::hooks::build_memories_context(memory_runtime.as_ref(), !options.defer_memory_warm)
        .await
        .unwrap_or_default();
    let injected_memory = if ctx.is_empty() { None } else { Some(ctx) };
    if options.defer_memory_warm {
        // Best-effort detached warm-up so the first turn's recall does not block on
        // embedder/store init. Bounded by the same startup lock timeout; errors are
        // ignored (the first turn re-opens the store on demand).
        let runtime_for_warm = Arc::clone(&memory_runtime);
        tokio::spawn(async move {
            let _ = tokio::time::timeout(
                crate::memory::runtime::MEMORY_STARTUP_LOCK_TIMEOUT,
                runtime_for_warm.ensure_store(),
            )
            .await;
        });
    }
    // Static (per-run) prompt knobs; `mode` is filled in per turn from `mode_state`.
    let prompt_options = CodingPromptOptions {
        mode: agent_mode,
        preferred_chat_language: options.settings.preferred_chat_language.clone(),
        ste_enabled: options.settings.simplified_technical_english,
        worker_name: worker_runtime.as_ref().map(|w| w.name.clone()),
        worker_peers: None,
        memory_enabled: options.settings.memory.enabled,
    };

    // Peers summary is refreshed each turn via registry (near-realtime demote first).
    let peers_registry = worker_runtime.as_ref().map(|w| w.registry());
    let peers_project_key = worker_runtime.as_ref().map(|w| w.project_key.clone());
    let peers_worker_id = worker_runtime.as_ref().map(|w| w.worker_id.clone());
    let peers_stale = worker_runtime.as_ref().map(|w| w.stale_secs()).unwrap_or(30);

    // Session continuity: todos/goals/last anchors — re-read each turn so restore and mid-session stay aligned.
    // Suppressed for brand-new sessions so the agent never inherits prior session state
    // unless the user explicitly resumes/continues via `--resume`, `--continue`, or `/resume`.
    let is_new_session = options.resume_id.is_none() || options.create_if_missing;
    let continuity_stores = ContinuityStores {
        session_id: session_id.clone(),
        todo_store: todo_store_for_prompt,
        goal_store: goal_store_for_prompt,
        is_new_session,
    };

    let system_prompt = if let Some(override_text) = options.system_prompt_override {
        // Even with a full override, append continuity so resume never loses open work.
        let continuity_stores = continuity_stores.clone();
        let override_text = override_text.to_string();
        SystemPrompt::Dynamic(Arc::new(move |ctx| {
            let base = override_text.clone();
            let continuity_stores = continuity_stores.clone();
            Box::pin(async move {
                let mut prompt = base;
                if let Some(section) = continuity_stores.build_section(ctx.session).await {
                    prompt.push_str("\n\n");
                    prompt.push_str(&section);
                }
                prompt
            })
        }))
    } else {
        let continuity_stores = continuity_stores.clone();
        SystemPrompt::Dynamic(Arc::new(move |ctx| {
            let cwd = cwd.clone();
            let agents_md = agents_md.clone();
            let mode_state = Arc::clone(&mode_for_prompt);
            let plan_reentry = Arc::clone(&plan_reentry_for_prompt);
            let memory_section = injected_memory.clone();
            let mut prompt_options = prompt_options.clone();
            let peers_registry = peers_registry.clone();
            let peers_project_key = peers_project_key.clone();
            let peers_worker_id = peers_worker_id.clone();
            let continuity_stores = continuity_stores.clone();
            Box::pin(async move {
                prompt_options.mode = *mode_state.lock().await;
                if let (Some(reg), Some(pk), Some(wid)) =
                    (peers_registry.as_ref(), peers_project_key.as_ref(), peers_worker_id.as_ref())
                    && let Ok(peers) = reg.list_live_peers(pk, wid, peers_stale).await
                {
                    let summary = peers
                        .into_iter()
                        .filter(|p| !p.is_self)
                        .map(|p| p.name)
                        .collect::<Vec<_>>()
                        .join(", ");
                    prompt_options.worker_peers = if summary.is_empty() { None } else { Some(summary) };
                }
                let tool_names: Vec<String> = ctx.active_tools.iter().map(|t| t.name().to_string()).collect();
                let mut prompt = build_coding_system_prompt(
                    &cwd,
                    &ctx.resources,
                    &tool_names,
                    agents_md.as_deref(),
                    &prompt_options,
                )
                .unwrap_or_else(|error| {
                    log::warn!("coding system prompt render failed: {error}");
                    elph_agent::DEFAULT_SYSTEM_PROMPT.to_string()
                });
                if prompt_options.mode == AgentMode::Plan && plan_reentry.load(Ordering::Relaxed) {
                    prompt.push_str(elph_agent::prompt::plan_mode_reentry_prompt());
                }

                // Append memory context section at the end of the system prompt.
                if let Some(ref mem) = memory_section {
                    prompt.push_str("\n\n");
                    prompt.push_str(mem);
                }

                // Structured resume state (todos / goal / last anchors).
                if let Some(section) = continuity_stores.build_section(ctx.session).await {
                    prompt.push_str("\n\n");
                    prompt.push_str(&section);
                }

                prompt
            })
        }))
    };

    let model = selection.model.clone();
    let models = Arc::clone(&selection.models);
    let compaction_settings = options.settings.compaction.to_agent_settings();
    // MCP tools are registered for execution/discovery but default-inactive: omit them
    // from the initial active set so empty/`None` restore does not activate every
    // connected MCP server's schemas. Session-tree `ActiveToolsChange` (lazy activation
    // or resume mid-session) still restores previously activated MCP names.
    let active_tool_names: Vec<String> = {
        let names: Vec<String> = tools
            .iter()
            .map(|t| t.name().to_string())
            .filter(|name| !is_mcp_tool(name))
            .collect();
        crate::platform::settings::apply::filter_default_tools(&names, options.settings.default_tools.as_deref())
    };
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

    if let Some(host) = options.extension_host {
        host.bind_to_harness(&harness).await;
    }

    // Wire the work tracker: increment on every successful mutating tool call so
    // `todo_write` can enforce that `completed` items actually did real work.
    let work_tracker_for_hook = work_tracker.clone();
    harness
        .on_tool_result(move |event: &elph_agent::harness::ToolResultEvent| {
            let tracker = work_tracker_for_hook.clone();
            let tool_name = event.tool_name.clone();
            let is_error = event.is_error;
            Box::pin(async move {
                if is_mutating_tool(&tool_name) && !is_error {
                    tracker.record_work();
                }
                None
            })
        })
        .await;

    // Post-turn todo hardening: close stale todos after a successful turn.
    // Best-effort — hook setup failures are logged, not fatal.
    if let Err(err) = crate::agent::todo_hooks::register_todo_auto_close_hook(
        &harness,
        Arc::clone(&todo_store),
        session_id.clone(),
        Arc::clone(&work_tracker),
        todo_hook.clone(),
    )
    .await
    {
        log::warn!("todo auto-close hook: {err:#}");
    }

    // Wire session_compact event: upsert the compaction summary into
    // `session_summaries` so other sessions can recall past context.
    // Runs best-effort — lock errors and write failures are logged, not fatal.
    let summary_store_for_hook = Arc::clone(&summary_store);
    let session_id_for_hook = session_id.clone();
    harness
        .on("session_compact", move |event| {
            let store = Arc::clone(&summary_store_for_hook);
            let session_id = session_id_for_hook.clone();
            Box::pin(async move {
                let compact = match &event {
                    elph_agent::harness::AgentHarnessOwnEvent::SessionCompact(e) => &e.compaction_entry,
                    _ => return None,
                };
                let fields = compact.as_compaction()?;
                let details_str = fields.details.map(|v| serde_json::to_string(v).unwrap_or_default());
                store
                    .upsert_best_effort(
                        &session_id,
                        fields.summary,
                        fields.tokens_before as i64,
                        Some(fields.first_kept_entry_id),
                        details_str.as_deref(),
                    )
                    .await;
                None
            })
        })
        .await
        .ok();

    // Rehydrate todos into the TUI immediately on restore/continue so the panel
    // and model-facing state match the durable `session_todos` rows.
    match continuity_stores.todo_store.list(&session_id).await {
        Ok(items) if !items.is_empty() => {
            let _ = ui_tx.send(crate::agent::AgentUiEvent::TodoUpdated { items });
        }
        Ok(_) => {}
        Err(err) => log::warn!("todo rehydrate on open failed: {err:#}"),
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
        ste_enabled: options.settings.simplified_technical_english,
        memory_enabled: options.settings.memory.enabled,
        worker_runtime,
        plan_reentry,
        default_tools: options.settings.default_tools.clone(),
    })
    .await?;

    if !options.defer_mcp_load {
        start_mcp_notifications(&session, Arc::clone(&mcp_registry), mcp_config_warnings);
    }

    Ok((session, ui_rx))
}

/// Stores used to build `<session_state>` for system prompt continuity on resume.
#[derive(Clone)]
struct ContinuityStores {
    session_id: String,
    todo_store: Arc<TodoStore>,
    goal_store: Arc<GoalStore>,
    /// When true, the session is brand-new — suppress the continuity brief entirely.
    is_new_session: bool,
}

impl ContinuityStores {
    async fn build_section<S>(&self, session: elph_agent::session::Session<S>) -> Option<String>
    where
        S: elph_agent::session::SessionStorage + Clone + Send + Sync + 'static,
    {
        // New sessions must never receive prior session state. Only resume/continue do.
        if self.is_new_session {
            return None;
        }
        let branch = session.branch(None).await.unwrap_or_default();
        let todos = self.todo_store.list(&self.session_id).await.unwrap_or_default();
        let goal = self.goal_store.get_latest_goal(&self.session_id).await.ok().flatten();
        let snap =
            super::session_continuity::ContinuitySnapshot::from_parts(&self.session_id, &branch, &todos, goal.as_ref());
        let section = snap.render();
        if section.is_empty() { None } else { Some(section) }
    }
}

/// Default display name for a worker: memorable-id (e.g. `calm-fox`), with hostname fallback.
fn default_worker_name() -> String {
    match memorable_ids::generate(memorable_ids::GenerateOptions::default()) {
        Ok(name) => {
            let name = name.trim().to_string();
            if name.is_empty() {
                hostname_worker_name_fallback()
            } else {
                name
            }
        }
        Err(_) => hostname_worker_name_fallback(),
    }
}

fn hostname_worker_name_fallback() -> String {
    std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("COMPUTERNAME"))
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "worker".into())
}

/// Returns `true` if the tool performs a mutating action that counts as "work".
///
/// Read-only tools (read_file, grep, find_path, list_dir, web_search, etc.) do
/// not advance progress. Mutating tools (edit_file, write_file, shell_exec,
/// delete_path, create_dir, move/copy, agent spawn, etc.) do.
fn is_mutating_tool(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "edit_file"
            | "write_file"
            | "delete_path"
            | "create_dir"
            | "move_path"
            | "copy_path"
            | "shell_exec"
            | "shell_use"
            | "spawn_agent"
            | "followup_task"
            | "request_mode_change"
    )
}
