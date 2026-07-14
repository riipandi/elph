//! List available tools — meta tool that describes all tools the agent can use.

use elph_ai::Tool;

use serde_json::json;

use crate::tools::simple_tool;
use crate::types::{AgentTool, AgentToolResult};

/// Create the `list_available_tools` tool from a snapshot of the current tool list.
///
/// The snapshot is captured at creation time. When MCP hot-reload changes the tool
/// set, the harness recreates tools via `set_tools`, which refreshes this snapshot.
pub fn create_list_available_tools(tools: &[AgentTool]) -> AgentTool {
    // Build a concise description list from the snapshot.
    let entries: Vec<serde_json::Value> = tools
        .iter()
        .map(|t| {
            let params = &t.tool.parameters;
            let required = params
                .get("required")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();

            json!({
                "name": t.tool.name,
                "description": t.tool.description,
                "parameters": params,
                "required": required,
            })
        })
        .collect();

    let catalog = serde_json::to_string_pretty(&entries).unwrap_or_else(|_| "[]".into());

    simple_tool(
        Tool {
            name: "list_available_tools".into(),
            description:
                "Lists all available tools that the agent can use, including their descriptions and usage instructions."
                    .into(),
            parameters: json!({
                "type": "object",
                "properties": {}
            }),
        },
        "list_available_tools",
        move |_, _| {
            let data = catalog.clone();
            Box::pin(async move { Ok(AgentToolResult::text(data)) })
        },
    )
}
