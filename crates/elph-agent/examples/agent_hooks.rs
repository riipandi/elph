//! Custom hooks — before/after tool call validation and logging.
//!
//! Uses faux provider to demonstrate `before_tool_call` and `after_tool_call` hooks.
//!
//! ```bash
//! cargo run -p elph-agent --example agent_hooks
//! ```

use std::sync::Arc;

use elph_agent::{
    AfterToolCallContext, AfterToolCallResult, Agent, AgentEvent, AgentOptions, BeforeToolCallContext,
    BeforeToolCallResult, PartialAgentState, simple_tool,
};
use elph_ai::{FauxResponseStep, Tool, faux_assistant_message, faux_provider, faux_text, faux_tool_call};
use serde_json::json;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let faux = faux_provider(Default::default());
    let model = faux.provider.get_models()[0].clone();
    faux.set_responses(vec![
        FauxResponseStep::Static(faux_assistant_message(
            vec![faux_tool_call("read_file", json!({"path": "/etc/passwd"}), None)],
            None,
        )),
        FauxResponseStep::Static(faux_assistant_message(
            vec![faux_tool_call("read_file", json!({"path": "Cargo.toml"}), None)],
            None,
        )),
        FauxResponseStep::Static(faux_assistant_message(
            vec![faux_text("Done! Read the file successfully.")],
            None,
        )),
        FauxResponseStep::Static(faux_assistant_message(
            vec![faux_text("Also read Cargo.toml successfully.")],
            None,
        )),
    ]);

    let provider = faux.provider.clone();
    let stream_fn: elph_agent::StreamFn = Arc::new(move |m, ctx, opts| provider.stream_simple(m, ctx, opts));

    let agent = Agent::new(AgentOptions {
        initial_state: Some(PartialAgentState {
            system_prompt: Some("You are a file reader.".into()),
            model: Some(model),
            tools: Some(vec![create_read_file_tool()]),
            ..Default::default()
        }),
        stream_fn: Some(stream_fn),
        before_tool_call: Some(Arc::new(|ctx, _signal| {
            Box::pin(async move { before_tool_hook(ctx).await })
        })),
        after_tool_call: Some(Arc::new(|ctx, _signal| {
            Box::pin(async move { after_tool_hook(ctx).await })
        })),
        ..Default::default()
    });

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

    println!("=== Before hook blocks /etc/passwd ===");
    agent.prompt_text("Read /etc/passwd", None).await?;
    agent.wait_for_idle().await;
    println!();
    println!();

    println!("=== After hook logs Cargo.toml ===");
    agent.prompt_text("Now read Cargo.toml", None).await?;
    agent.wait_for_idle().await;
    println!();

    Ok(())
}

async fn before_tool_hook(ctx: BeforeToolCallContext) -> Option<BeforeToolCallResult> {
    if ctx.tool_call.name == "read_file"
        && let Some(path) = ctx.args.get("path").and_then(|p| p.as_str())
    {
        if path.starts_with("/etc") || path.contains("password") {
            println!("  [before_hook] BLOCKED: {path}");
            return Some(BeforeToolCallResult {
                block: true,
                reason: Some(format!("Access to '{path}' blocked by security policy")),
                args: None,
            });
        }
        println!("  [before_hook] ALLOWED: {path}");
    }
    None
}

async fn after_tool_hook(ctx: AfterToolCallContext) -> Option<AfterToolCallResult> {
    if ctx.tool_call.name == "read_file" && !ctx.is_error {
        let size: usize = ctx
            .result
            .content
            .iter()
            .map(|c| match c {
                elph_agent::ToolResultContent::Text(t) => t.text.len(),
                _ => 0,
            })
            .sum();
        println!("  [after_hook] read_file succeeded, {size} chars");
    }
    None
}

fn create_read_file_tool() -> elph_agent::AgentTool {
    simple_tool(
        Tool {
            name: "read_file".into(),
            description: "Read the contents of a file".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "File path" }
                },
                "required": ["path"]
            }),
        },
        "Read File",
        |_, args| {
            let path = args.get("path").and_then(|p| p.as_str()).unwrap_or("").to_string();
            Box::pin(async move {
                if path == "Cargo.toml" {
                    Ok(elph_agent::AgentToolResult::text(
                        "[package]\nname = \"elph-rs\"\nversion = \"0.0.22\"",
                    ))
                } else {
                    Ok(elph_agent::AgentToolResult::text(format!(
                        "Contents of {path} (simulated)"
                    )))
                }
            })
        },
    )
}
