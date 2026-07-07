use std::sync::Arc;

use elph_agent::{Agent, AgentMessage, AgentOptions, PartialAgentState, llm_message_to_agent};
use elph_ai::{FauxResponseStep, Message, faux_assistant_message, faux_provider, faux_text};

#[tokio::test]
async fn agent_prompt_updates_state() {
    let faux = faux_provider(Default::default());
    let model = faux.provider.get_models()[0].clone();
    faux.set_responses(vec![FauxResponseStep::Static(faux_assistant_message(
        vec![faux_text("ok")],
        None,
    ))]);

    let stream_fn: elph_agent::StreamFn = {
        let provider = faux.provider.clone();
        Arc::new(move |m, ctx, opts| provider.stream_simple(m, ctx, opts))
    };

    let agent = Agent::new(AgentOptions {
        initial_state: Some(PartialAgentState {
            model: Some(model),
            ..Default::default()
        }),
        stream_fn: Some(stream_fn),
        ..Default::default()
    });

    agent.prompt_text("hello", None).await.expect("prompt should succeed");
    agent.wait_for_idle().await;

    let state = agent.state().await;
    assert_eq!(state.messages.len(), 2);
    assert!(matches!(state.messages[0], AgentMessage::Llm(_)));
    assert_eq!(state.messages[0].role(), "user");
}

#[tokio::test]
async fn agent_steering_queue_drains_one_at_a_time() {
    let faux = faux_provider(Default::default());
    let model = faux.provider.get_models()[0].clone();
    faux.set_responses(vec![
        FauxResponseStep::Static(faux_assistant_message(vec![faux_text("first")], None)),
        FauxResponseStep::Static(faux_assistant_message(vec![faux_text("second")], None)),
    ]);

    let stream_fn: elph_agent::StreamFn = {
        let provider = faux.provider.clone();
        Arc::new(move |m, ctx, opts| provider.stream_simple(m, ctx, opts))
    };

    let agent = Agent::new(AgentOptions {
        initial_state: Some(PartialAgentState {
            model: Some(model),
            ..Default::default()
        }),
        stream_fn: Some(stream_fn),
        ..Default::default()
    });

    agent.steer(llm_message_to_agent(Message::User {
        content: elph_ai::UserContent::Text("steer-one".into()),
        timestamp: 1,
    }));
    agent.steer(llm_message_to_agent(Message::User {
        content: elph_ai::UserContent::Text("steer-two".into()),
        timestamp: 2,
    }));
    assert!(agent.has_queued_messages());

    agent.prompt_text("start", None).await.expect("prompt should succeed");
    agent.wait_for_idle().await;

    assert!(!agent.has_queued_messages());
}
