//! Agent event lifecycle — observe every phase of agent execution.
//!
//! Demonstrates: `AgentEvent` variants, `subscribe()` / `unsubscribe()`,
//! `TurnStart`/`TurnEnd`, `MessageStart`/`MessageUpdate`/`MessageEnd`,
//! `ToolExecutionStart`/`ToolExecutionEnd`.
//!
//! ```bash
//! cargo run -p elph-agent --example agent_events
//! ```

use std::sync::Arc;

use elph_agent::{Agent, AgentEvent, AgentOptions, PartialAgentState};
use elph_ai::{FauxResponseStep, faux_assistant_message, faux_provider, faux_text, faux_tool_call};
use serde_json::json;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let faux = faux_provider(Default::default());
    let model = faux.provider.get_models()[0].clone();
    faux.set_responses(vec![
        FauxResponseStep::Static(faux_assistant_message(
            vec![faux_tool_call("echo", json!({"text": "hi"}), None)],
            None,
        )),
        FauxResponseStep::Static(faux_assistant_message(vec![faux_text("Echo complete.")], None)),
    ]);

    let provider = faux.provider.clone();
    let stream_fn: elph_agent::StreamFn = Arc::new(move |m, ctx, opts| provider.stream_simple(m, ctx, opts));

    let agent = Agent::new(AgentOptions {
        initial_state: Some(PartialAgentState {
            system_prompt: Some("Echo everything.".into()),
            model: Some(model),
            tools: Some(vec![elph_agent::echo_tool()]),
            ..Default::default()
        }),
        stream_fn: Some(stream_fn),
        ..Default::default()
    });

    // ── Subscribe and log all events ──
    let subscription = agent
        .subscribe(Arc::new(|event, _token| {
            Box::pin(async move {
                match event {
                    AgentEvent::AgentStart => println!("[agent] start"),
                    AgentEvent::AgentEnd { .. } => println!("[agent] end"),
                    AgentEvent::TurnStart => println!("  [turn] start"),
                    AgentEvent::TurnEnd { .. } => println!("  [turn] end"),
                    AgentEvent::MessageStart { .. } => println!("    [msg] start"),
                    AgentEvent::MessageUpdate {
                        assistant_message_event,
                        ..
                    } => {
                        if let elph_ai::AssistantMessageEvent::TextDelta { delta, .. } = *assistant_message_event {
                            print!("{delta}");
                        }
                    }
                    AgentEvent::MessageEnd { .. } => println!("    [msg] end"),
                    AgentEvent::ToolExecutionStart { tool_name, .. } => {
                        println!("    [tool] start: {tool_name}");
                    }
                    AgentEvent::ToolExecutionEnd {
                        tool_name, is_error, ..
                    } => {
                        println!("    [tool] end: {tool_name} (error={is_error})");
                    }
                    _ => {}
                }
            })
        }))
        .await;

    // ── Run ──
    agent.prompt_text("Echo something", None).await?;
    agent.wait_for_idle().await;
    println!();

    // ── Unsubscribe after first run ──
    subscription.unsubscribe().await;
    println!("Unsubscribed — no more events printed.");

    Ok(())
}
