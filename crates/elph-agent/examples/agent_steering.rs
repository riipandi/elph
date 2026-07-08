//! Steering messages — redirect the agent mid-execution.
//!
//! Uses faux provider to demonstrate `steer()` and `follow_up()`.
//!
//! ```bash
//! cargo run -p elph-agent --example agent_steering
//! ```

use std::sync::Arc;

use elph_agent::{Agent, AgentEvent, AgentOptions, PartialAgentState, llm_message_to_agent};
use elph_ai::{FauxResponseStep, UserContent, faux_assistant_message, faux_provider, faux_text};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // ── Faux provider with queued responses ──
    let faux = faux_provider(Default::default());
    let model = faux.provider.get_models()[0].clone();
    faux.set_responses(vec![
        // Turn 1: initial response
        FauxResponseStep::Static(faux_assistant_message(
            vec![faux_text("I'll help you with that task. Let me start...")],
            None,
        )),
        // Turn 2: after steering — changed direction
        FauxResponseStep::Static(faux_assistant_message(
            vec![faux_text(
                "Understood! Switching to the new task: writing a Rust hello world.",
            )],
            None,
        )),
        // Turn 3: after follow-up — additional work
        FauxResponseStep::Static(faux_assistant_message(
            vec![faux_text(
                "Here's the hello world program:\n\nfn main() {\n    println!(\"Hello, world!\");\n}",
            )],
            None,
        )),
    ]);

    let provider = faux.provider.clone();
    let stream_fn: elph_agent::StreamFn = Arc::new(move |m, ctx, opts| provider.stream_simple(m, ctx, opts));

    // ── Build agent ──
    let agent = Agent::new(AgentOptions {
        initial_state: Some(PartialAgentState {
            system_prompt: Some("You are a helpful assistant.".into()),
            model: Some(model),
            ..Default::default()
        }),
        stream_fn: Some(stream_fn),
        ..Default::default()
    });

    // ── Subscribe to events ──
    agent
        .subscribe(Arc::new(|event, _token| {
            Box::pin(async move {
                if let AgentEvent::MessageUpdate {
                    assistant_message_event,
                    ..
                } = event
                    && let elph_ai::AssistantMessageEvent::TextDelta { delta, .. } = *assistant_message_event
                {
                    print!("{delta}");
                }
            })
        }))
        .await;

    // ── Turn 1: Initial prompt ──
    println!("═══ Turn 1: Initial Prompt ═══");
    agent.prompt_text("Help me debug this Rust code", None).await?;
    agent.wait_for_idle().await;
    println!();
    println!();

    // ── Steering: redirect mid-execution ──
    println!("═══ Steering: Change Direction ═══");
    println!("User steers: \"Actually, forget that. Write a hello world program instead.\"");
    agent.steer(llm_message_to_agent(elph_ai::Message::User {
        content: UserContent::Text("Actually, forget that. Write a hello world program instead.".into()),
        timestamp: now_ms(),
    }));
    agent.continue_run().await?;
    agent.wait_for_idle().await;
    println!();
    println!();

    // ── Follow-up: additional request ──
    println!("═══ Follow-up: Add Comments ═══");
    println!("User follows up: \"Add comments explaining each line.\"");
    agent.follow_up(llm_message_to_agent(elph_ai::Message::User {
        content: UserContent::Text("Add comments explaining each line.".into()),
        timestamp: now_ms(),
    }));
    agent.continue_run().await?;
    agent.wait_for_idle().await;
    println!();

    // ── Show state ──
    let state = agent.state().await;
    println!();
    println!("═══ Summary ═══");
    println!("Messages: {}", state.messages.len());
    println!("Queued: {}", agent.has_queued_messages());

    Ok(())
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}
