//! Faux agent example — prompt a mock provider and print streamed text.
//!
//! Run with:
//!   cargo run -p elph-agent --example faux_agent

use std::sync::Arc;

use elph_agent::{Agent, AgentEvent, AgentOptions, PartialAgentState};
use elph_ai::{FauxResponseStep, faux_assistant_message, faux_provider, faux_text};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let faux = faux_provider(Default::default());
    let model = faux.provider.get_models()[0].clone();
    faux.set_responses(vec![FauxResponseStep::Static(faux_assistant_message(
        vec![faux_text("Hello from elph-agent!")],
        None,
    ))]);

    let provider = faux.provider.clone();
    let stream_fn: elph_agent::StreamFn = Arc::new(move |m, ctx, opts| provider.stream_simple(m, ctx, opts));

    let agent = Agent::new(AgentOptions {
        initial_state: Some(PartialAgentState {
            system_prompt: Some("You are a helpful assistant.".into()),
            model: Some(model),
            ..Default::default()
        }),
        stream_fn: Some(stream_fn),
        ..Default::default()
    });

    agent
        .subscribe(Arc::new(|event, _token| {
            Box::pin(async move {
                if let AgentEvent::MessageUpdate {
                    assistant_message_event,
                    ..
                } = event
                    && let elph_ai::AssistantMessageEvent::TextDelta { delta, .. } = &*assistant_message_event
                {
                    print!("{delta}");
                }
            })
        }))
        .await;

    print!("assistant: ");
    agent.prompt_text("Say hello.", None).await?;
    agent.wait_for_idle().await;
    println!();

    let state = agent.state().await;
    println!("transcript messages: {}", state.messages.len());
    Ok(())
}
