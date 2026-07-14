//! Search tools demo — read_file, grep, find_path, list_dir.
//!
//! Uses the faux provider (no API key needed) to demonstrate the Read & Search
//! tool group. Shows how the agent reads files, searches content, finds files
//! by glob, and lists directories.
//!
//! ```bash
//! cargo run -p elph-agent --features builtin-tools --example agent_search_tools
//! ```

use std::sync::Arc;

use elph_agent::{Agent, AgentEvent, AgentOptions, LocalExecutionEnv, PartialAgentState, create_search_tools};
use elph_ai::{FauxResponseStep, faux_assistant_message, faux_provider, faux_text, faux_tool_call};
use serde_json::json;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cwd = std::env::current_dir()?;
    let env = Arc::new(LocalExecutionEnv::new(&cwd));

    // ── 1. Faux provider: sequence of search tool calls ──
    let faux = faux_provider(Default::default());
    let model = faux.provider.get_models()[0].clone();

    faux.set_responses(vec![
        // Turn 1: list the root directory
        FauxResponseStep::Static(faux_assistant_message(
            vec![faux_tool_call("list_dir", json!({ "path": "." }), None)],
            None,
        )),
        FauxResponseStep::Static(faux_assistant_message(
            vec![faux_text("Listed root directory contents. Now let me read Cargo.toml.")],
            None,
        )),
        // Turn 2: read a file
        FauxResponseStep::Static(faux_assistant_message(
            vec![faux_tool_call(
                "read_file",
                json!({ "path": "Cargo.toml", "limit": 20 }),
                None,
            )],
            None,
        )),
        FauxResponseStep::Static(faux_assistant_message(
            vec![faux_text(
                "Read the first 20 lines of Cargo.toml. Now searching for Rust files.",
            )],
            None,
        )),
        // Turn 3: find all .rs files
        FauxResponseStep::Static(faux_assistant_message(
            vec![faux_tool_call(
                "find_path",
                json!({ "pattern": "*.toml", "limit": 10 }),
                None,
            )],
            None,
        )),
        FauxResponseStep::Static(faux_assistant_message(
            vec![faux_text("Found TOML files. Now searching for 'serde' in Cargo.toml.")],
            None,
        )),
        // Turn 4: grep for a pattern
        FauxResponseStep::Static(faux_assistant_message(
            vec![faux_tool_call(
                "grep",
                json!({ "pattern": "elph-agent", "path": "Cargo.toml", "literal": true }),
                None,
            )],
            None,
        )),
        FauxResponseStep::Static(faux_assistant_message(
            vec![faux_text(
                "Found all references to elph-agent. Search tools demo complete!",
            )],
            None,
        )),
    ]);

    let provider = faux.provider.clone();
    let stream_fn: elph_agent::StreamFn = Arc::new(move |m, ctx, opts| provider.stream_simple(m, ctx, opts));

    // ── 2. Build agent with Search Tools only ──
    let agent_tools = create_search_tools(env.clone());
    let tool_names: Vec<&str> = agent_tools.iter().map(|t| t.name()).collect();
    println!("Tools available: {tool_names:?}");
    println!();

    let agent = Agent::new(AgentOptions {
        initial_state: Some(PartialAgentState {
            system_prompt: Some(
                "You are a code exploration assistant. Use the available search tools \
                 to explore the project structure, read files, search for patterns, \
                 and find files by glob patterns."
                    .into(),
            ),
            model: Some(model),
            tools: Some(agent_tools),
            ..Default::default()
        }),
        stream_fn: Some(stream_fn),
        ..Default::default()
    });

    // ── 3. Subscribe to events ──
    agent
        .subscribe(Arc::new(move |event, _token| {
            Box::pin(async move {
                match event {
                    AgentEvent::ToolExecutionStart { tool_name, args, .. } => {
                        println!("  [tool] {tool_name}({args})");
                    }
                    AgentEvent::ToolExecutionEnd {
                        tool_name,
                        is_error,
                        result,
                        ..
                    } => {
                        let status = if is_error { "ERR" } else { "OK" };
                        let output: String = result
                            .content
                            .iter()
                            .filter_map(|b| match b {
                                elph_agent::ToolResultContent::Text(t) => Some(t.text.as_str()),
                                _ => None,
                            })
                            .collect();
                        // Truncate long output for display
                        let preview = if output.len() > 200 {
                            format!("{}...", &output[..200])
                        } else {
                            output
                        };
                        println!("  [{status}] {tool_name}: {preview}");
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

    // ── 4. Run ──
    println!("=== Search Tools Demo ===");
    println!();
    agent
        .prompt_text(
            "Explore this project: list the root directory, read Cargo.toml, \
             find all .toml files, and search for 'elph-agent' in Cargo.toml.",
            None,
        )
        .await?;
    agent.wait_for_idle().await;
    println!();

    // ── 5. Summary ──
    let state = agent.state().await;
    println!("Messages: {}", state.messages.len());

    Ok(())
}
