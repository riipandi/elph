//! Custom tools, steering, and follow-up with the faux provider — no API key needed.
//!
//! Demonstrates: `simple_tool`, `echo_tool`, custom `AgentTool`, `steer()`, `follow_up()`,
//! `wait_for_idle()`, `abort()`, and `AgentEvent` subscription.
//!
//! ```bash
//! cargo run -p elph-agent --example agent_tools
//! ```

use std::sync::Arc;

use elph_agent::{Agent, AgentEvent, AgentOptions, AgentToolResult, PartialAgentState, ToolExecutionMode, simple_tool};
use elph_ai::{FauxResponseStep, Tool, faux_assistant_message, faux_provider, faux_text, faux_tool_call};
use serde_json::json;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // ── 1. Faux provider with queued responses ──
    let faux = faux_provider(Default::default());
    let model = faux.provider.get_models()[0].clone();
    faux.set_responses(vec![
        FauxResponseStep::Static(faux_assistant_message(
            vec![faux_tool_call("echo", json!({"text": "Hello from tool!"}), None)],
            None,
        )),
        FauxResponseStep::Static(faux_assistant_message(vec![faux_text("Done: echoed text")], None)),
        FauxResponseStep::Static(faux_assistant_message(
            vec![faux_text("Understood, switching to Rust mode.")],
            None,
        )),
    ]);

    let provider = faux.provider.clone();
    let stream_fn: elph_agent::StreamFn = Arc::new(move |m, ctx, opts| provider.stream_simple(m, ctx, opts));

    // ── 2. Build agent with tools ──
    let agent = Agent::new(AgentOptions {
        initial_state: Some(PartialAgentState {
            system_prompt: Some("You are a helpful assistant with access to tools.".into()),
            model: Some(model),
            tools: Some(vec![
                elph_agent::echo_tool(),
                simple_tool(
                    Tool {
                        name: "uppercase".into(),
                        description: "Convert text to uppercase".into(),
                        parameters: json!({
                            "type": "object",
                            "properties": { "text": { "type": "string" } },
                            "required": ["text"]
                        }),
                    },
                    "Uppercase",
                    |_, args| {
                        let text = args.get("text").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        Box::pin(async move { Ok(AgentToolResult::text(text.to_uppercase())) })
                    },
                ),
            ]),
            ..Default::default()
        }),
        stream_fn: Some(stream_fn),
        tool_execution: ToolExecutionMode::Sequential,
        ..Default::default()
    });

    // ── 3. Subscribe to events ──
    let event_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    {
        let event_count = event_count.clone();
        agent
            .subscribe(Arc::new(move |event, _token| {
                let event_count = event_count.clone();
                Box::pin(async move {
                    match &event {
                        AgentEvent::ToolExecutionStart { tool_name, .. } => {
                            println!("  [tool] start: {tool_name}");
                        }
                        AgentEvent::ToolExecutionEnd {
                            tool_name, is_error, ..
                        } => {
                            println!("  [tool] end: {tool_name} (error={is_error})");
                        }
                        AgentEvent::TurnEnd { .. } => {
                            println!("  [turn] ended");
                        }
                        _ => {}
                    }
                    event_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                })
            }))
            .await;
    }

    // ── 4. Prompt with tool call ──
    println!("=== Prompt: echo tool call ===");
    agent.prompt_text("Echo the greeting", None).await?;
    agent.wait_for_idle().await;

    println!();

    // ── 5. Steering: inject a mid-turn message ──
    println!("=== Steering: inject mid-turn message ===");
    agent.steer(elph_agent::llm_message_to_agent(elph_ai::Message::User {
        content: elph_ai::UserContent::Text("Switch to Rust mode now".into()),
        timestamp: now_ms(),
    }));
    agent.wait_for_idle().await;

    println!();

    // ── 6. Final state ──
    let state = agent.state().await;
    println!("Total messages: {}", state.messages.len());
    println!(
        "Total events captured: {}",
        event_count.load(std::sync::atomic::Ordering::SeqCst)
    );

    Ok(())
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}
