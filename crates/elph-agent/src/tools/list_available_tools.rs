//! List available tools — meta tool that describes all tools the agent can use.

use elph_ai::Tool;

use serde_json::json;

use crate::tools::simple_tool;
use crate::types::{AgentTool, AgentToolResult};

/// Create the `list_available_tools` tool from a snapshot of the current tool list.
///
/// The snapshot is captured at creation time. When MCP hot-reload changes the tool
/// set, the harness recreates tools via `set_tools`, which refreshes this snapshot.
///
/// The model may pass an optional `name_prefix` argument to narrow the result to
/// tools whose name starts with that substring (e.g. `mcp_github__` to fetch just
/// one MCP server's schemas). Omitting it lists the entire catalog (backward
/// compatible with the prompt line "`list_available_tools` only when you need
/// details about an unfamiliar or dynamically added tool").
pub fn create_list_available_tools(tools: &[AgentTool]) -> AgentTool {
    let snapshot: Vec<AgentTool> = tools.to_vec();

    simple_tool(
        Tool {
            name: "list_available_tools".into(),
            constrained_sampling: None,
            description:
                "Lists available tools that the agent can use, including their descriptions and usage instructions. "
                    .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "name_prefix": {
                        "type": "string",
                        "description": "Optional prefix to filter tool names (e.g. an MCP server name); omit to list everything."
                    }
                },
                "additionalProperties": false
            }),
        },
        "list_available_tools",
        move |_, args| {
            let snapshot = snapshot.clone();
            Box::pin(async move {
                let prefix = args
                    .get("name_prefix")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned)
                    .filter(|p| !p.is_empty());
                let entries: Vec<serde_json::Value> = snapshot
                    .iter()
                    .filter(|t| prefix.as_deref().is_none_or(|p| t.tool.name.starts_with(p)))
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
                let data = serde_json::to_string_pretty(&entries).unwrap_or_else(|_| "[]".into());
                Ok(AgentToolResult::text(data))
            })
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool(name: &str) -> AgentTool {
        simple_tool(
            Tool {
                name: name.into(),
                constrained_sampling: None,
                description: format!("{name} tool"),
                parameters: json!({"type": "object", "properties": {}}),
            },
            name,
            |_, _| Box::pin(async move { Ok(AgentToolResult::text("ok")) }),
        )
    }

    fn snapshot_tools() -> Vec<AgentTool> {
        vec![
            tool("read_file"),
            tool("grep"),
            tool("mcp_github__list_issues"),
            tool("mcp_github__get_issue"),
            tool("mcp_browser__navigate"),
            tool("mcp_browser__click"),
            tool("spawn_agent"),
        ]
    }

    fn output_text(meta: &AgentTool, args: serde_json::Value) -> String {
        let env = std::sync::Arc::new(crate::runtime::local_env::LocalExecutionEnv::new("/tmp"));
        let ctx = crate::tools::types::ToolContext::new(env);
        let fut = (meta.execute)(String::new(), args, None, None, ctx);
        let result = futures::executor::block_on(fut).expect("runs");
        match result.content.first().expect("content") {
            crate::tools::types::ToolResultContent::Text(t) => t.text.clone(),
            _ => panic!("expected text"),
        }
    }

    #[test]
    fn no_arg_returns_everything() {
        let meta = create_list_available_tools(&snapshot_tools());
        let text = output_text(&meta, json!({}));
        assert!(text.contains("read_file"));
        assert!(text.contains("mcp_github__list_issues"));
        assert!(text.contains("mcp_browser__click"));
        assert!(text.contains("spawn_agent"));
    }

    #[test]
    fn name_prefix_filters_mcp_server() {
        let meta = create_list_available_tools(&snapshot_tools());
        let text = output_text(&meta, json!({"name_prefix": "mcp_github__"}));
        assert!(text.contains("mcp_github__list_issues"));
        assert!(text.contains("mcp_github__get_issue"));
        assert!(!text.contains("read_file"));
        assert!(!text.contains("mcp_browser__navigate"));
        assert!(!text.contains("spawn_agent"));
    }

    #[test]
    fn name_prefix_empty_behaves_like_no_arg() {
        let meta = create_list_available_tools(&snapshot_tools());
        let text = output_text(&meta, json!({"name_prefix": ""}));
        assert!(text.contains("read_file"));
        assert!(text.contains("mcp_browser__click"));
        assert!(text.contains("spawn_agent"));
    }
}
