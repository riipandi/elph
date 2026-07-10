//! Subagent control plane tests.
mod common;

use std::sync::Arc;

use elph_agent::{
    AgentControl, AgentGraphStore, AgentHarnessResources, AgentHarnessStreamOptions, LocalExecutionEnv,
    SubagentBootstrap, SubagentLimits, SubagentSpawnConfig, SubagentStatus, create_read_only_tools,
};
use elph_agent::{Migration, ensure_database};

const GRAPH_MIGRATION: &[Migration] = &[Migration {
    version: 7,
    name: "create_agent_spawn_edges_table",
    up: "CREATE TABLE IF NOT EXISTS agent_spawn_edges (
            parent_session_id TEXT NOT NULL,
            child_session_id TEXT NOT NULL,
            agent_path TEXT NOT NULL,
            depth INTEGER NOT NULL,
            status TEXT NOT NULL DEFAULT 'open',
            created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
            PRIMARY KEY (parent_session_id, child_session_id)
        );",
}];

#[tokio::test(flavor = "multi_thread")]
async fn spawn_and_list_subagents_with_session_dir() {
    let temp = tempfile::TempDir::new().expect("tempdir");
    let env = Arc::new(LocalExecutionEnv::new(temp.path()));
    let (faux, models) = common::new_faux();
    let stream_fn = common::faux_stream_fn(&faux);
    let tools = create_read_only_tools(env.clone());

    let sessions_root = temp.path().join("sessions").to_string_lossy().to_string();
    std::fs::create_dir_all(&sessions_root).expect("sessions root");

    let graph_db = temp.path().join("metadata.db");
    ensure_database(&graph_db, GRAPH_MIGRATION)
        .await
        .expect("graph migrate");

    let bootstrap = SubagentBootstrap {
        project_key: "testproj".into(),
        cwd: temp.path().to_string_lossy().to_string(),
        sessions_root,
        resources: AgentHarnessResources::default(),
        stream_options: AgentHarnessStreamOptions::default(),
        thinking_level: Default::default(),
        agent_graph: Some(Arc::new(AgentGraphStore::new(&graph_db))),
    };

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
        Arc::new(elph_agent::AgentRegistry::new()),
        "root",
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
    assert_eq!(agents[0].agent_path, "root/review");
    assert_eq!(agents[0].depth, 1);
    assert!(!agents[0].session_id.is_empty());
    assert!(matches!(
        agents[0].status,
        SubagentStatus::Done | SubagentStatus::Idle | SubagentStatus::Running
    ));

    let child_dir = temp.path().join("sessions/testproj");
    assert!(child_dir.exists(), "project session dir should exist");
}
