//! Filesystem tools demo — create_dir, copy_path, delete_path, move_path.
//!
//! Uses the faux provider (no API key needed) to demonstrate the Edit Tools
//! that operate on directories and paths.
//!
//! ```bash
//! cargo run -p elph-agent --features builtin-tools --example agent_filesystem_tools
//! ```

use std::sync::Arc;

use elph_agent::{Agent, AgentEvent, AgentOptions, LocalExecutionEnv, PartialAgentState, create_edit_tools};
use elph_ai::{FauxResponseStep, faux_assistant_message, faux_provider, faux_text, faux_tool_call};
use serde_json::json;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cwd = std::env::current_dir()?;
    let env = Arc::new(LocalExecutionEnv::new(&cwd));

    // ── 1. Faux provider with tool calls ──
    let faux = faux_provider(Default::default());
    let model = faux.provider.get_models()[0].clone();

    faux.set_responses(vec![
        // Turn 1: create a directory structure
        FauxResponseStep::Static(faux_assistant_message(
            vec![faux_tool_call(
                "create_dir",
                json!({ "path": "demo_output/src" }),
                None,
            )],
            None,
        )),
        FauxResponseStep::Static(faux_assistant_message(
            vec![faux_text("Created demo_output/src directory.")],
            None,
        )),
        // Turn 2: write a file then copy it
        FauxResponseStep::Static(faux_assistant_message(
            vec![faux_tool_call(
                "write_file",
                json!({ "path": "demo_output/src/main.rs", "content": "fn main() { println!(\"Hello from demo!\"); }\n" }),
                None,
            )],
            None,
        )),
        FauxResponseStep::Static(faux_assistant_message(
            vec![faux_text("Wrote main.rs. Now copying it.")],
            None,
        )),
        FauxResponseStep::Static(faux_assistant_message(
            vec![faux_tool_call(
                "copy_path",
                json!({ "source": "demo_output/src/main.rs", "destination": "demo_output/src/main_backup.rs" }),
                None,
            )],
            None,
        )),
        FauxResponseStep::Static(faux_assistant_message(
            vec![faux_text("Copied main.rs to main_backup.rs. Now renaming it.")],
            None,
        )),
        // Turn 3: move/rename the backup
        FauxResponseStep::Static(faux_assistant_message(
            vec![faux_tool_call(
                "move_path",
                json!({ "source": "demo_output/src/main_backup.rs", "destination": "demo_output/src/renamed.rs" }),
                None,
            )],
            None,
        )),
        FauxResponseStep::Static(faux_assistant_message(
            vec![faux_text("Renamed to renamed.rs. Now cleaning up.")],
            None,
        )),
        // Turn 4: delete the demo directory
        FauxResponseStep::Static(faux_assistant_message(
            vec![faux_tool_call(
                "delete_path",
                json!({ "path": "demo_output" }),
                None,
            )],
            None,
        )),
        FauxResponseStep::Static(faux_assistant_message(
            vec![faux_text("Cleaned up demo_output. All filesystem tools demonstrated successfully!")],
            None,
        )),
    ]);

    let provider = faux.provider.clone();
    let stream_fn: elph_agent::StreamFn = Arc::new(move |m, ctx, opts| provider.stream_simple(m, ctx, opts));

    // ── 2. Build agent with Edit Tools ──
    let agent_tools = create_edit_tools(env.clone());
    let tool_names: Vec<&str> = agent_tools.iter().map(|t| t.name()).collect();
    println!("Tools available: {tool_names:?}");
    println!();

    let agent = Agent::new(AgentOptions {
        initial_state: Some(PartialAgentState {
            system_prompt: Some(
                "You are a filesystem assistant. Use the available tools to demonstrate \
                 directory creation, file copying, moving/renaming, and deletion."
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
                        println!("  [{status}] {tool_name}: {output}");
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
    println!("=== Filesystem Tools Demo ===");
    println!();
    agent
        .prompt_text(
            "Create a demo_output/src directory, write a main.rs file, copy it, \
             rename the copy, then delete the entire demo_output directory.",
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
