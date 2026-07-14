//! List available tools demo — introspect the agent tool catalog.
//!
//! Uses the faux provider (no API key needed) to demonstrate `list_available_tools`,
//! a meta tool that returns the full tool catalog with descriptions and parameters.
//!
//! ```bash
//! cargo run -p elph-agent --features builtin-tools --example agent_list_tools
//! ```

use std::sync::Arc;

use elph_agent::{Agent, AgentEvent, AgentOptions, BuiltinToolsBuilder, LocalExecutionEnv, PartialAgentState};
use elph_ai::{FauxResponseStep, faux_assistant_message, faux_provider, faux_text, faux_tool_call};
use serde_json::json;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cwd = std::env::current_dir()?;
    let env = Arc::new(LocalExecutionEnv::new(&cwd));

    // ── 1. Build full tool catalog ──
    let tools = BuiltinToolsBuilder::all(env.clone()).build();
    let tool_count = tools.len();
    let tool_names: Vec<&str> = tools.iter().map(|t| t.name()).collect();

    println!("Registered {tool_count} tools:");
    for name in &tool_names {
        println!("  - {name}");
    }
    println!();

    // ── 2. Faux provider: ask agent to list tools ──
    let faux = faux_provider(Default::default());
    let model = faux.provider.get_models()[0].clone();

    faux.set_responses(vec![
        FauxResponseStep::Static(faux_assistant_message(
            vec![faux_tool_call("list_available_tools", json!({}), None)],
            None,
        )),
        FauxResponseStep::Static(faux_assistant_message(
            vec![faux_text("I have listed all available tools above. Each tool includes its name, description, parameters, and required fields.")],
            None,
        )),
    ]);

    let provider = faux.provider.clone();
    let stream_fn: elph_agent::StreamFn = Arc::new(move |m, ctx, opts| provider.stream_simple(m, ctx, opts));

    // ── 3. Build agent ──
    let agent = Agent::new(AgentOptions {
        initial_state: Some(PartialAgentState {
            system_prompt: Some(
                "You are a helpful assistant. When asked about your capabilities, \
                 use the list_available_tools tool to show what you can do."
                    .into(),
            ),
            model: Some(model),
            tools: Some(tools),
            ..Default::default()
        }),
        stream_fn: Some(stream_fn),
        ..Default::default()
    });

    // ── 4. Subscribe to events ──
    agent
        .subscribe(Arc::new(move |event, _token| {
            Box::pin(async move {
                match event {
                    AgentEvent::ToolExecutionStart { tool_name, .. } => {
                        println!("  [tool] {tool_name}");
                    }
                    AgentEvent::ToolExecutionEnd { tool_name, result, .. } => {
                        // For list_available_tools, show a summary instead of full JSON
                        if tool_name == "list_available_tools" {
                            let text: String = result
                                .content
                                .iter()
                                .filter_map(|b| match b {
                                    elph_agent::ToolResultContent::Text(t) => Some(t.text.as_str()),
                                    _ => None,
                                })
                                .collect();
                            if let Ok(list) = serde_json::from_str::<Vec<serde_json::Value>>(&text) {
                                println!("  [OK] {tool_name}: {} tools listed", list.len());
                                for entry in &list {
                                    let name = entry.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                                    let desc = entry
                                        .get("description")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("")
                                        .chars()
                                        .take(80)
                                        .collect::<String>();
                                    println!("       - {name}: {desc}");
                                }
                            } else {
                                println!("  [OK] {tool_name}: {text}");
                            }
                        } else {
                            println!("  [OK] {tool_name}");
                        }
                    }
                    AgentEvent::MessageUpdate {
                        assistant_message_event,
                        ..
                    } => {
                        if let elph_ai::AssistantMessageEvent::TextDelta { delta, .. } = &*assistant_message_event {
                            print!("{delta}");
                        }
                    }
                    _ => {}
                }
            })
        }))
        .await;

    // ── 5. Run ──
    println!("=== List Available Tools Demo ===");
    println!();
    agent
        .prompt_text("What tools do you have available? List them all.", None)
        .await?;
    agent.wait_for_idle().await;
    println!();

    // ── 6. Summary ──
    let state = agent.state().await;
    println!("Messages: {}", state.messages.len());

    Ok(())
}
