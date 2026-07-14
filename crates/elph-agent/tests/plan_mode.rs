//! Plan mode policy and harness integration.
mod common;

use std::sync::Arc;

use elph_agent::{
    AgentHarness, AgentHarnessEvent, AgentHarnessOptions, AgentHarnessResources, AgentThinkingLevel,
    BuiltinToolsBuilder, CollaborationMode, InMemorySessionStorage, LocalExecutionEnv, PlanConfirmationChoice,
    QueueMode, Session, SystemPrompt, create_search_tools, extract_proposed_plan, plan_mode_blocks_tool,
};
use elph_ai::{FauxResponseStep, faux_assistant_message, faux_text};
use tempfile::TempDir;

fn test_env() -> (TempDir, Arc<LocalExecutionEnv>) {
    let temp = TempDir::new().expect("tempdir");
    let path = temp.path().to_path_buf();
    (temp, Arc::new(LocalExecutionEnv::new(&path)))
}

#[test]
fn extract_proposed_plan_parses_block() {
    let text = "Ready.\n<proposed_plan>\nStep 1\n</proposed_plan>";
    assert_eq!(extract_proposed_plan(text).as_deref(), Some("Step 1"));
}

#[test]
fn plan_mode_blocks_write_tool() {
    assert!(plan_mode_blocks_tool(CollaborationMode::Plan, "write_file"));
    assert!(!plan_mode_blocks_tool(CollaborationMode::Default, "write_file"));
}

#[tokio::test(flavor = "multi_thread")]
async fn harness_enter_plan_mode_filters_active_tools() {
    let (faux, models) = common::new_faux();
    let (_temp, env) = test_env();
    let session = Session::new(InMemorySessionStorage::new(None).expect("session"));
    let tools = BuiltinToolsBuilder::all(env.clone()).build();

    let harness = AgentHarness::new(AgentHarnessOptions {
        env,
        session,
        models,
        tools,
        resources: AgentHarnessResources::default(),
        system_prompt: SystemPrompt::Static("test".into()),
        stream_options: Default::default(),
        model: faux.provider.get_models()[0].clone(),
        thinking_level: AgentThinkingLevel::Off,
        active_tool_names: vec![],
        steering_mode: QueueMode::default(),
        follow_up_mode: QueueMode::default(),
        goal_runtime: None,
        subagent_bootstrap: None,
        shared_registry: None,
        agent_control: None,
    })
    .expect("harness");

    harness.enter_plan_mode().await.expect("enter plan");
    assert_eq!(harness.collaboration_mode().await, CollaborationMode::Plan);
    let active: Vec<_> = harness
        .get_active_tools()
        .await
        .into_iter()
        .map(|t| t.name().to_string())
        .collect();
    assert!(active.contains(&"read_file".to_string()));
    assert!(!active.contains(&"write_file".to_string()));
    assert!(!active.contains(&"bash".to_string()));
}

#[tokio::test(flavor = "multi_thread")]
async fn harness_emits_plan_confirmation_events() {
    let plan_body = "## Plan\nDo the thing";
    let (faux, models) = common::new_faux();
    faux.set_responses(vec![FauxResponseStep::Static(faux_assistant_message(
        vec![faux_text(format!(
            "Here is the plan:\n<proposed_plan>\n{plan_body}\n</proposed_plan>"
        ))],
        None,
    ))]);
    let model = faux.provider.get_models()[0].clone();

    let (_temp, env) = test_env();
    let session = Session::new(InMemorySessionStorage::new(None).expect("session"));
    let tools = create_search_tools(env.clone());

    let harness = AgentHarness::new(AgentHarnessOptions {
        env,
        session,
        models,
        tools,
        resources: AgentHarnessResources::default(),
        system_prompt: SystemPrompt::Static("test".into()),
        stream_options: Default::default(),
        model: model.clone(),
        thinking_level: AgentThinkingLevel::Off,
        active_tool_names: vec![],
        steering_mode: QueueMode::default(),
        follow_up_mode: QueueMode::default(),
        goal_runtime: None,
        subagent_bootstrap: None,
        shared_registry: None,
        agent_control: None,
    })
    .expect("harness");

    harness.enter_plan_mode().await.expect("plan mode");

    let events = Arc::new(tokio::sync::Mutex::new(Vec::new()));
    let events_clone = events.clone();
    harness
        .subscribe(move |event, _| {
            let events = events_clone.clone();
            Box::pin(async move {
                if let AgentHarnessEvent::Agent(agent_event) = event {
                    events.lock().await.push(format!("{agent_event:?}"));
                }
            })
        })
        .await;

    harness.prompt("Plan a feature", None).await.expect("prompt");
    harness.wait_for_idle().await.expect("idle");

    let captured = events.lock().await;
    assert!(captured.iter().any(|e| e.contains("PlanProposed")));
    assert!(captured.iter().any(|e| e.contains("PlanConfirmationRequired")));

    harness
        .resolve_plan_confirmation(PlanConfirmationChoice::StayInPlan)
        .await
        .expect("resolve");
    assert_eq!(harness.collaboration_mode().await, CollaborationMode::Plan);
}
