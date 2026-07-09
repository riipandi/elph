//! Subagent control plane tests.

mod common;

use std::sync::Arc;

use elph_agent::{
    AgentControl, LocalExecutionEnv, SubagentLimits, SubagentSpawnConfig, SubagentStatus, create_read_only_tools,
};
#[tokio::test]
async fn spawn_and_list_subagents() {
    let temp = tempfile::TempDir::new().expect("tempdir");
    let env = Arc::new(LocalExecutionEnv::new(temp.path()));
    let (faux, models) = common::new_faux();
    let stream_fn = common::faux_stream_fn(&faux);
    let tools = create_read_only_tools(env.clone());

    let control = AgentControl::new(
        SubagentSpawnConfig {
            env,
            model: faux.provider.get_models()[0].clone(),
            system_prompt: "subagent".into(),
            tools,
            stream_fn,
            parent_session_id: "parent".into(),
        },
        SubagentLimits::default(),
        0,
    );

    let id = control
        .spawn_agent("review", Some("Review the module".into()))
        .await
        .expect("spawn");
    control.wait_agent(&id).await.expect("wait");

    let agents = control.list_agents().await;
    assert_eq!(agents.len(), 1);
    assert_eq!(agents[0].id, id);
    assert_eq!(agents[0].task_name, "review");
    assert!(matches!(
        agents[0].status,
        SubagentStatus::Done | SubagentStatus::Idle | SubagentStatus::Running
    ));
    let _ = (faux, models);
}
