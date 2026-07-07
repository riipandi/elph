//! Built-in agent tools and tool registry helpers.

mod bash;
mod common;
mod edit;
mod find;
mod grep;
mod ls;
mod read;
mod write;

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use elph_ai::Tool;
use serde_json::Value;

use crate::harness::types::ExecutionEnv;
use crate::types::{AgentTool, AgentToolResult, ToolExecuteFn};

pub use bash::create_bash_tool;
pub use edit::create_edit_tool;
pub use find::create_find_tool;
pub use grep::create_grep_tool;
pub use ls::create_ls_tool;
pub use read::create_read_tool;
pub use write::create_write_tool;

pub fn simple_tool(
    tool: Tool,
    label: impl Into<String>,
    execute: impl Fn(String, Value) -> Pin<Box<dyn Future<Output = anyhow::Result<AgentToolResult>> + Send>>
    + Send
    + Sync
    + 'static,
) -> AgentTool {
    let execute_fn: ToolExecuteFn = Arc::new(move |id, args, _signal, _on_update| execute(id, args));
    AgentTool {
        tool,
        label: label.into(),
        execution_mode: None,
        prepare_arguments: None,
        execute: execute_fn,
    }
}

pub fn echo_tool() -> AgentTool {
    simple_tool(
        Tool {
            name: "echo".into(),
            description: "Echo text".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": { "text": { "type": "string" } },
                "required": ["text"]
            }),
        },
        "Echo",
        |_, args| {
            let text = args.get("text").and_then(|v| v.as_str()).unwrap_or("").to_string();
            Box::pin(async move { Ok(AgentToolResult::text(text)) })
        },
    )
}

/// Core coding tools: read, bash, edit, write.
pub fn create_coding_tools(env: Arc<dyn ExecutionEnv>) -> Vec<AgentTool> {
    vec![
        create_read_tool(env.clone()),
        create_bash_tool(env.clone()),
        create_edit_tool(env.clone()),
        create_write_tool(env),
    ]
}

/// Read-only exploration tools.
pub fn create_read_only_tools(env: Arc<dyn ExecutionEnv>) -> Vec<AgentTool> {
    vec![
        create_read_tool(env.clone()),
        create_grep_tool(env.clone()),
        create_find_tool(env.clone()),
        create_ls_tool(env),
    ]
}

/// All built-in tools.
pub fn create_all_tools(env: Arc<dyn ExecutionEnv>) -> Vec<AgentTool> {
    vec![
        create_read_tool(env.clone()),
        create_bash_tool(env.clone()),
        create_edit_tool(env.clone()),
        create_write_tool(env.clone()),
        create_grep_tool(env.clone()),
        create_find_tool(env.clone()),
        create_ls_tool(env),
    ]
}
