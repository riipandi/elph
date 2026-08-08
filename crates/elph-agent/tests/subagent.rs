//! Subagent control plane tests.
mod common;

use std::sync::Arc;

use elph_agent::AgentControl;
use elph_agent::AgentGraphStore;
use elph_agent::AgentHarnessResources;
use elph_agent::AgentHarnessStreamOptions;
use elph_agent::LocalExecutionEnv;
use elph_agent::Migration;
use elph_agent::SubagentBootstrap;
use elph_agent::SubagentLimits;
use elph_agent::SubagentSpawnConfig;
use elph_agent::SubagentStatus;
use elph_agent::create_search_tools;
use elph_agent::ensure_database;
use elph_ai::{FauxResponseStep, StopReason};
use elph_ai::{faux_assistant_message, faux_text};

const PLATFORM_LIKE: &[Migration] = &[
    Migration {
        version: 7,
        name: "create_agent_spawn_edges_table",
        up: "CREATE TABLE IF NOT EXISTS agent_spawn_edges (
            parent_session_id TEXT NOT NULL,
            child_session_id TEXT NOT NULL,
            agent_path TEXT NOT NULL,
            depth INTEGER NOT NULL,
            status TEXT NOT NULL DEFAULT 'open',
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            PRIMARY KEY (parent_session_id, child_session_id)
        ) STRICT;",
    },
    Migration {
        version: 100,
        name: "session_tree_pi_schema",
        up: elph_agent::SESSION_TREE_MIGRATIONS[0].up,
    },
];

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
    ensure_database(&graph_db, PLATFORM_LIKE)
        .await
        .expect("platform migrate");

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

    let registry = Arc::new(elph_agent::AgentRegistry::new());
    let parent_path = elph_agent::generate_agent_name();
    let model_a = faux.provider.get_models()[0].clone();
    let control = AgentControl::new(
        SubagentSpawnConfig {
            env,
            model: model_a.clone(),
            system_prompt: "subagent".into(),
            base_tools: tools,
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
    let opened = elph_agent::TursoSessionRepo::new(&graph_db)
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
    ensure_database(&graph_db, PLATFORM_LIKE)
        .await
        .expect("platform migrate");

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

    let registry = Arc::new(elph_agent::AgentRegistry::new());
    let parent_path = elph_agent::generate_agent_name();
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
    ensure_database(&graph_db, PLATFORM_LIKE)
        .await
        .expect("platform migrate");

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

    let registry = Arc::new(elph_agent::AgentRegistry::new());
    let parent_path = elph_agent::generate_agent_name();
    let control = AgentControl::new(
        SubagentSpawnConfig {
            env,
            model: faux.provider.get_models()[0].clone(),
            system_prompt: "subagent".into(),
            base_tools: tools,
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
