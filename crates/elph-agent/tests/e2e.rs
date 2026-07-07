//! Basic end-to-end harness tests.

use std::sync::Arc;

use elph_agent::{
    AgentHarness, AgentHarnessOptions, AgentHarnessResources, InMemorySessionStorage, LocalExecutionEnv, Session,
    SystemPrompt,
};
use elph_ai::{FauxResponseStep, builtin_models, faux_assistant_message, faux_provider, faux_text};
use tempfile::TempDir;

#[tokio::test]
async fn harness_prompt_persists_session_messages() {
    let temp = TempDir::new().expect("temp dir");
    let env = Arc::new(LocalExecutionEnv::new(temp.path()));
    let faux = faux_provider(Default::default());
    let mut models = builtin_models(None);
    models.set_provider(faux.provider.clone());
    let models = models.into_arc();
    let model = faux.provider.get_models()[0].clone();
    faux.set_responses(vec![FauxResponseStep::Static(faux_assistant_message(
        vec![faux_text("4")],
        None,
    ))]);

    let session = Session::new(InMemorySessionStorage::new(None).expect("session"));
    let harness = AgentHarness::new(AgentHarnessOptions {
        env,
        session,
        models,
        tools: vec![],
        resources: AgentHarnessResources::default(),
        system_prompt: SystemPrompt::Static("You are a helpful assistant.".into()),
        stream_options: Default::default(),
        model,
        thinking_level: Default::default(),
        active_tool_names: vec![],
        steering_mode: Default::default(),
        follow_up_mode: Default::default(),
    })
    .expect("harness");

    let response = harness.prompt("What is 2+2?", None).await.expect("prompt");
    assert_eq!(response.role, "assistant");
    harness.wait_for_idle().await.expect("idle");
    assert!(
        response
            .content
            .iter()
            .any(|block| matches!(block, elph_ai::AssistantContentBlock::Text(text) if text.text.contains('4')))
    );
}
