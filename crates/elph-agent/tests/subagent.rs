//! Subagent control plane tests.
mod common;

use std::sync::Arc;

use elph_agent::agent::subagent::AgentControl;
use elph_agent::agent::subagent::AgentGraphStore;
use elph_agent::agent::subagent::SubagentBootstrap;
use elph_agent::agent::subagent::SubagentLimits;
use elph_agent::agent::subagent::SubagentSpawnConfig;
use elph_agent::agent::subagent::SubagentStatus;
use elph_agent::create_search_tools;
use elph_agent::datastore::ensure_database;
use elph_agent::harness::AgentHarnessResources;
use elph_agent::harness::AgentHarnessStreamOptions;
use elph_agent::runtime::LocalExecutionEnv;
use elph_agent::session::SESSION_TREE_MIGRATIONS;
use elph_ai::{FauxResponseStep, StopReason, faux_assistant_message, faux_text, faux_thinking};

/// Parent session row required by FK on `agent_spawn_edges` / child sessions.
async fn seed_parent_session(db_path: &std::path::Path, session_id: &str) {
    let db = elph_agent::datastore::open_local(db_path).await.expect("open");
    let conn = elph_agent::datastore::connect(&db).await.expect("connect");
    conn.execute(
        "INSERT INTO sessions (id, created_at, updated_at, cwd) VALUES (?, ?, ?, ?)",
        turso::params![session_id, "2026-01-01T00:00:00Z", "2026-01-01T00:00:00Z", "/tmp"],
    )
    .await
    .expect("seed parent session");
}

#[tokio::test(flavor = "multi_thread")]
async fn spawn_and_list_subagents_with_turso_sessions() {
    let temp = tempfile::TempDir::new().expect("tempdir");
    let env = Arc::new(LocalExecutionEnv::new(temp.path()));
    let (faux, models) = common::new_faux();
    faux.set_responses(vec![FauxResponseStep::Static(faux_assistant_message(
        vec![faux_text("Review complete.")],
        Some(StopReason::Stop),
    ))]);
    let stream_fn = common::faux_stream_fn(&faux);
    let tools = create_search_tools(env.clone());

    let graph_db = temp.path().join("store.db");
    ensure_database(&graph_db, &SESSION_TREE_MIGRATIONS)
        .await
        .expect("platform migrate");
    seed_parent_session(&graph_db, "parent_sess").await;

    let bootstrap = SubagentBootstrap {
        cwd: temp.path().to_string_lossy().to_string(),
        store_db_path: graph_db.to_string_lossy().to_string(),
        resources: AgentHarnessResources::default(),
        stream_options: AgentHarnessStreamOptions::default(),
        thinking_level: Default::default(),
        prompt_encoding: None,
        database: None,
        agent_graph: Some(Arc::new(AgentGraphStore::new(&graph_db))),
        outputs_root: Some(temp.path().join("outputs")),
    };

    let registry = Arc::new(elph_agent::agent::subagent::AgentRegistry::new());
    let parent_path = elph_agent::agent::subagent::generate_agent_name();
    let model_a = faux.provider.get_models()[0].clone();
    let control = AgentControl::new(
        SubagentSpawnConfig {
            env,
            model: model_a.clone(),
            system_prompt: "subagent".into(),
            base_tools: tools,
            active_tool_names: vec![],
            stream_fn,
            models,
            root_session_id: "parent_sess".into(),
            bootstrap: Some(bootstrap),
        },
        SubagentLimits::default(),
        0,
        registry,
        parent_path.clone(),
    );

    let id = control
        .spawn_agent("review", Some("Review the module".into()))
        .await
        .expect("spawn");
    control.wait_agent(&id).await.expect("wait");

    let agents = control.list_agents(None).await;
    assert_eq!(agents.len(), 1);
    assert_eq!(agents[0].id, id);
    assert_eq!(agents[0].task_name, "review");
    assert_eq!(agents[0].agent_path, format!("{}/{}", parent_path, agents[0].id));
    assert!(
        agents[0].id.starts_with("agent_"),
        "subagent id should use agent_ prefix, got {}",
        agents[0].id
    );
    assert_ne!(agents[0].id, parent_path, "subagent id should differ from parent");
    assert_eq!(agents[0].depth, 1);
    assert!(!agents[0].session_id.is_empty());
    assert!(matches!(
        agents[0].status,
        SubagentStatus::Done | SubagentStatus::Idle | SubagentStatus::Running
    ));

    // Output summary is populated with the final assistant text (not "no output").
    assert_eq!(agents[0].output.text, "Review complete.");
    assert!(agents[0].output.turns >= 1);

    // Persistent artifacts are written under outputs_root/subagents/<agent_id>/.
    let agent_dir = temp.path().join("outputs").join("subagents").join(&agents[0].id);
    assert!(agent_dir.join("output.md").exists(), "output.md missing");
    let output_md = std::fs::read_to_string(agent_dir.join("output.md")).expect("read output.md");
    assert_eq!(output_md.trim(), "Review complete.");
    assert!(agent_dir.join("meta.json").exists(), "meta.json missing");
    let meta_json = std::fs::read_to_string(agent_dir.join("meta.json")).expect("read meta.json");
    let meta: serde_json::Value = serde_json::from_str(&meta_json).expect("meta.json is valid JSON");
    let expected_model = format!("{}/{}", model_a.provider, model_a.id);
    assert_eq!(
        meta["model"].as_str(),
        Some(expected_model.as_str()),
        "meta.json must record provider_id/model_id",
    );
    assert_eq!(agents[0].model, expected_model, "list_agents must report provider_id/model_id");

    // Child session is durable in the shared Turso DB (not SessionDir).
    let child_session_id = agents[0].session_id.clone();
    let opened = elph_agent::session::TursoSessionRepo::new(&graph_db)
        .open(&child_session_id)
        .await
        .expect("open child session from store.db");
    assert_eq!(opened.metadata().await.id, child_session_id);
}

/// A subagent must inherit the parent harness's *current* active model, not the
/// model that was configured when the parent `AgentControl` was constructed.
/// Regression test for subagents silently falling back to `defaultModel`.
#[tokio::test(flavor = "multi_thread")]
async fn subagent_inherits_current_model_after_switch() {
    let temp = tempfile::TempDir::new().expect("tempdir");
    let env = Arc::new(LocalExecutionEnv::new(temp.path()));
    let (faux, models) = common::new_faux();
    faux.set_responses(vec![FauxResponseStep::Static(faux_assistant_message(
        vec![faux_text("ok")],
        Some(StopReason::Stop),
    ))]);
    let stream_fn = common::faux_stream_fn(&faux);
    let tools = create_search_tools(env.clone());

    let graph_db = temp.path().join("store.db");
    ensure_database(&graph_db, &SESSION_TREE_MIGRATIONS)
        .await
        .expect("platform migrate");
    seed_parent_session(&graph_db, "parent_sess").await;

    let bootstrap = SubagentBootstrap {
        cwd: temp.path().to_string_lossy().to_string(),
        store_db_path: graph_db.to_string_lossy().to_string(),
        resources: AgentHarnessResources::default(),
        stream_options: AgentHarnessStreamOptions::default(),
        thinking_level: Default::default(),
        prompt_encoding: None,
        database: None,
        agent_graph: Some(Arc::new(AgentGraphStore::new(&graph_db))),
        outputs_root: Some(temp.path().join("outputs")),
    };

    let registry = Arc::new(elph_agent::agent::subagent::AgentRegistry::new());
    let parent_path = elph_agent::agent::subagent::generate_agent_name();
    let model_a = faux.provider.get_models()[0].clone();
    let mut model_b = model_a.clone();
    model_b.id = "switched-model".to_string();
    model_b.name = "Switched Model".to_string();

    let control = AgentControl::new(
        SubagentSpawnConfig {
            env,
            model: model_a.clone(),
            system_prompt: "subagent".into(),
            base_tools: tools,
            active_tool_names: vec![],
            stream_fn,
            models,
            root_session_id: "parent_sess".into(),
            bootstrap: Some(bootstrap),
        },
        SubagentLimits::default(),
        0,
        registry,
        parent_path,
    );

    // First subagent uses the construction-time model.
    let id1 = control
        .spawn_agent("first", Some("do first".into()))
        .await
        .expect("spawn");
    control.wait_agent(&id1).await.expect("wait");
    let used1 = control.subagent_harness(&id1).await.unwrap().model().await;
    assert_eq!(used1.id, model_a.id, "first subagent should use the initially configured model");

    // Switch the active model, then spawn again — the new subagent must follow it.
    control.set_model(model_b.clone()).await;
    let id2 = control
        .spawn_agent("second", Some("do second".into()))
        .await
        .expect("spawn");
    control.wait_agent(&id2).await.expect("wait");
    let used2 = control.subagent_harness(&id2).await.unwrap().model().await;
    assert_eq!(
        used2.id, model_b.id,
        "second subagent must inherit the switched active model, not the construction-time default"
    );
}

/// Rapid `followup_task` + `wait_agent` interleaving must never hang or return
/// before the dispatched turn starts (the historical "no output" flake).
#[tokio::test(flavor = "multi_thread")]
async fn wait_immediately_after_followup_never_races_turn_start() {
    let temp = tempfile::TempDir::new().expect("tempdir");
    let env = Arc::new(LocalExecutionEnv::new(temp.path()));
    let (faux, models) = common::new_faux();
    // One response per dispatched turn (8 followups + margin). The faux queue is
    // consumed per stream call, so enqueue a batch of identical Static steps.
    let queued = std::iter::repeat_n(
        faux_assistant_message(vec![faux_text("Turn complete.")], Some(StopReason::Stop)),
        12,
    )
    .map(FauxResponseStep::Static)
    .collect::<Vec<_>>();
    faux.append_responses(queued);
    let stream_fn = common::faux_stream_fn(&faux);
    let tools = create_search_tools(env.clone());

    let graph_db = temp.path().join("store.db");
    ensure_database(&graph_db, &SESSION_TREE_MIGRATIONS)
        .await
        .expect("platform migrate");
    seed_parent_session(&graph_db, "parent_sess").await;

    let bootstrap = SubagentBootstrap {
        cwd: temp.path().to_string_lossy().to_string(),
        store_db_path: graph_db.to_string_lossy().to_string(),
        resources: AgentHarnessResources::default(),
        stream_options: AgentHarnessStreamOptions::default(),
        thinking_level: Default::default(),
        prompt_encoding: None,
        database: None,
        agent_graph: Some(Arc::new(AgentGraphStore::new(&graph_db))),
        outputs_root: Some(temp.path().join("outputs")),
    };

    let registry = Arc::new(elph_agent::agent::subagent::AgentRegistry::new());
    let parent_path = elph_agent::agent::subagent::generate_agent_name();
    let control = AgentControl::new(
        SubagentSpawnConfig {
            env,
            model: faux.provider.get_models()[0].clone(),
            system_prompt: "subagent".into(),
            base_tools: tools,
            active_tool_names: vec![],
            stream_fn,
            models,
            root_session_id: "parent_sess".into(),
            bootstrap: Some(bootstrap),
        },
        SubagentLimits::default(),
        0,
        registry,
        parent_path.clone(),
    );

    let id = control.spawn_agent("race", None).await.expect("spawn");

    // Interleave a bare wait right after a followup dispatch — the followup
    // must be observed as in-flight even though its task may not have started.
    let mut summaries = Vec::new();
    for _ in 0..8 {
        control
            .followup_task(&id, "Do work".into())
            .await
            .expect("followup dispatch");
        let summary = control.wait_agent_for_output(&id).await.expect("wait");
        summaries.push(summary);
    }

    // Every wait must have returned non-empty output (never a hang, never a
    // "no output captured" placeholder from a missed run).
    let agents = control.list_agents(None).await;
    assert_eq!(agents.len(), 1);
    assert_eq!(agents[0].output.text, "Turn complete.");
    assert!(agents[0].output.turns >= 8);
    assert!(!summaries.iter().any(|s| s.contains("no output")));
}

/// A subagent whose final assistant message contains only Thinking blocks
/// (no Text blocks) must still persist its output to output.md.
/// Regression test for subagent output.md being empty when models return
/// thinking-only responses.
#[tokio::test(flavor = "multi_thread")]
async fn subagent_persists_thinking_only_response_to_output_md() {
    let temp = tempfile::TempDir::new().expect("tempdir");
    let env = Arc::new(LocalExecutionEnv::new(temp.path()));
    let (faux, models) = common::new_faux();
    faux.set_responses(vec![FauxResponseStep::Static(faux_assistant_message(
        vec![faux_thinking("I will analyze the code.")],
        Some(StopReason::Stop),
    ))]);
    let stream_fn = common::faux_stream_fn(&faux);
    let tools = create_search_tools(env.clone());

    let graph_db = temp.path().join("store.db");
    ensure_database(&graph_db, &SESSION_TREE_MIGRATIONS)
        .await
        .expect("platform migrate");
    seed_parent_session(&graph_db, "parent_sess").await;

    let bootstrap = SubagentBootstrap {
        cwd: temp.path().to_string_lossy().to_string(),
        store_db_path: graph_db.to_string_lossy().to_string(),
        resources: AgentHarnessResources::default(),
        stream_options: AgentHarnessStreamOptions::default(),
        thinking_level: Default::default(),
        prompt_encoding: None,
        database: None,
        agent_graph: Some(Arc::new(AgentGraphStore::new(&graph_db))),
        outputs_root: Some(temp.path().join("outputs")),
    };

    let registry = Arc::new(elph_agent::agent::subagent::AgentRegistry::new());
    let parent_path = elph_agent::agent::subagent::generate_agent_name();
    let control = AgentControl::new(
        SubagentSpawnConfig {
            env,
            model: faux.provider.get_models()[0].clone(),
            system_prompt: "subagent".into(),
            base_tools: tools,
            active_tool_names: vec![],
            stream_fn,
            models,
            root_session_id: "parent_sess".into(),
            bootstrap: Some(bootstrap),
        },
        SubagentLimits::default(),
        0,
        registry,
        parent_path.clone(),
    );

    let id = control
        .spawn_agent("thinker", Some("Think about this".into()))
        .await
        .expect("spawn");
    control.wait_agent(&id).await.expect("wait");

    let agents = control.list_agents(None).await;
    assert_eq!(agents.len(), 1);
    assert_eq!(agents[0].output.turns, 1);

    // output.md must contain the thinking content, not be empty.
    let agent_dir = temp.path().join("outputs").join("subagents").join(&agents[0].id);
    assert!(agent_dir.join("output.md").exists(), "output.md missing");
    let output_md = std::fs::read_to_string(agent_dir.join("output.md")).expect("read output.md");
    assert_eq!(output_md.trim(), "I will analyze the code.");
}
