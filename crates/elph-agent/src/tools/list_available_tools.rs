//! List available tools — meta tool that describes all tools the agent can use.
//!
//! The catalog is serialized to compact XML with `quick-xml` (serde `serialize`
//! feature): attributes carry the schema type / required / enum, element text
//! carries the description. XML is deliberately used over JSON — it is
//! token-cheaper and models parse it as easily as the `<available_skills>`
//! system-prompt block.

use elph_ai::Tool;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use serde_json::json;

use crate::tools::simple_tool;
use crate::types::{AgentTool, AgentToolResult};

/// Strip characters XML 1.0 forbids (e.g. NUL) from text that flows into element
/// values or attribute values; tab, newline, and CR are kept. `quick-xml` escapes
/// `&`, `<`, `>` but passes control characters through raw, so without this the
/// emitted document could be malformed for strict parsers.
fn xml_clean(value: &str) -> String {
    value
        .chars()
        .filter(|c| {
            matches!(
                c,
                '\u{9}' | '\u{A}' | '\u{D}'
                    | '\u{20}'..='\u{D7FF}'
                    | '\u{E000}'..='\u{FFFD}'
                    | '\u{10000}'..='\u{10FFFF}'
            )
        })
        .collect()
}

/// Serde model of `<available_tools>`; serialized via `quick_xml::se`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct ToolCatalog {
    #[serde(rename = "tool")]
    tools: Vec<ToolEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct ToolEntry {
    #[serde(rename = "@name")]
    name: String,
    #[serde(rename = "@params", skip_serializing_if = "Option::is_none")]
    params: Option<String>,
    #[serde(rename = "$text")]
    description: String,
}

/// Build the `<available_tools>` XML catalog for a tool snapshot.
fn format_tool_catalog(tools: &[AgentTool]) -> String {
    let catalog = ToolCatalog {
        tools: tools
            .iter()
            .map(|tool| {
                let params = tool
                    .tool
                    .parameters
                    .as_object()
                    .and_then(|obj| obj.get("properties"))
                    .and_then(Value::as_object)
                    .filter(|props| !props.is_empty())
                    .map(|props| {
                        let names: Vec<String> = props.keys().cloned().collect();
                        names.join(",")
                    });
                ToolEntry {
                    name: xml_clean(&tool.tool.name),
                    params,
                    description: xml_clean(&tool.tool.description),
                }
            })
            .collect(),
    };
    match quick_xml::se::to_string_with_root("available_tools", &catalog) {
        Ok(xml) => xml,
        Err(_) => "<available_tools></available_tools>".to_string(),
    }
}

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
            description: "Lists tools the agent can discover, including full parameter schemas. \
MCP tools (names starting with `mcp_`) are inactive until activated: pass `name_prefix` \
(e.g. `mcp_deepwiki__`) to filter, return schemas, and activate matched tools for later turns. \
Omit `name_prefix` to browse the full catalog without activating."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "name_prefix": {
                        "type": "string",
                        "description": "Optional name prefix filter (e.g. `mcp_deepwiki__`). When set, matched tools are activated for subsequent turns via added_tool_names. Omit to list everything without activating."
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
                let matched: Vec<AgentTool> = snapshot
                    .iter()
                    .filter(|t| prefix.as_deref().is_none_or(|p| t.tool.name.starts_with(p)))
                    .cloned()
                    .collect();
                let data = format_tool_catalog(&matched);
                // When a `name_prefix` was used, advertise the matched tool names so
                // the harness can lazily activate them (their schemas become usable
                // from the next turn — see `AgentHarness::activate_lazy_tools`).
                let added = prefix
                    .as_deref()
                    .map(|_| matched.iter().map(|t| t.tool.name.clone()).collect::<Vec<_>>());
                Ok(AgentToolResult {
                    content: vec![crate::types::ToolResultContent::Text(elph_ai::TextContent::new(data))],
                    details: json!({}),
                    added_tool_names: added,
                    terminate: None,
                    usage: None,
                })
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

    fn tool_with_params(name: &str, params: Value) -> AgentTool {
        simple_tool(
            Tool {
                name: name.into(),
                constrained_sampling: None,
                description: format!("{name} tool"),
                parameters: params,
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

    fn output_result(meta: &AgentTool, args: serde_json::Value) -> AgentToolResult {
        let env = std::sync::Arc::new(crate::runtime::local_env::LocalExecutionEnv::new("/tmp"));
        let ctx = crate::tools::types::ToolContext::new(env);
        let fut = (meta.execute)(String::new(), args, None, None, ctx);
        futures::executor::block_on(fut).expect("runs")
    }

    fn output_text(meta: &AgentTool, args: serde_json::Value) -> String {
        let result = output_result(meta, args);
        match result.content.first().expect("content") {
            crate::tools::types::ToolResultContent::Text(t) => t.text.clone(),
            _ => panic!("expected text"),
        }
    }

    #[test]
    fn no_arg_returns_everything_as_xml() {
        let meta = create_list_available_tools(&snapshot_tools());
        let text = output_text(&meta, json!({}));
        assert!(text.starts_with("<available_tools>"), "{text}");
        assert!(text.ends_with("</available_tools>"), "{text}");
        assert!(text.contains("<tool name=\"read_file\""));
        assert!(text.contains("<tool name=\"mcp_github__list_issues\""));
        assert!(text.contains("<tool name=\"mcp_browser__click\""));
        assert!(text.contains("<tool name=\"spawn_agent\""));
    }

    #[test]
    fn name_prefix_filters_mcp_server() {
        let meta = create_list_available_tools(&snapshot_tools());
        let result = output_result(&meta, json!({"name_prefix": "mcp_github__"}));
        let text = match result.content.first().expect("content") {
            crate::tools::types::ToolResultContent::Text(t) => t.text.clone(),
            _ => panic!("expected text"),
        };
        assert!(text.contains("<tool name=\"mcp_github__list_issues\""));
        assert!(text.contains("<tool name=\"mcp_github__get_issue\""));
        assert!(!text.contains("<tool name=\"read_file\""));
        assert!(!text.contains("<tool name=\"mcp_browser__navigate\""));
        assert!(!text.contains("<tool name=\"spawn_agent\""));
        // With a prefix, the matched tool names are advertised so the harness
        // can lazily activate them for the next turn.
        let added = result.added_tool_names.expect("advertised");
        assert!(added.contains(&"mcp_github__list_issues".to_string()));
        assert!(added.contains(&"mcp_github__get_issue".to_string()));
        assert!(!added.contains(&"mcp_browser__navigate".to_string()));
        assert!(!added.contains(&"read_file".to_string()));
    }

    #[test]
    fn no_prefix_does_not_advertise() {
        let meta = create_list_available_tools(&snapshot_tools());
        let result = output_result(&meta, json!({"name_prefix": ""}));
        assert!(result.added_tool_names.is_none());
        let no_arg = output_result(&meta, json!({}));
        assert!(no_arg.added_tool_names.is_none());
    }

    #[test]
    fn name_prefix_empty_behaves_like_no_arg() {
        let meta = create_list_available_tools(&snapshot_tools());
        let text = output_text(&meta, json!({"name_prefix": ""}));
        assert!(text.contains("<tool name=\"read_file\""));
        assert!(text.contains("<tool name=\"mcp_browser__click\""));
        assert!(text.contains("<tool name=\"spawn_agent\""));
    }

    #[test]
    fn tools_with_parameters_show_params_attribute() {
        let params = json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "File path" },
                "limit": { "type": "number" }
            },
            "required": ["path"]
        });
        let meta = create_list_available_tools(&[tool_with_params("sample_tool", params)]);
        let text = output_text(&meta, json!({}));
        // Keys are sorted alphabetically
        assert!(text.contains("<tool name=\"sample_tool\" params=\"path,limit\""));
    }

    #[test]
    fn tools_without_properties_omit_params_attribute() {
        let meta = create_list_available_tools(&[tool("bare")]);
        let text = output_text(&meta, json!({}));
        assert!(text.contains("<tool name=\"bare\""));
        assert!(!text.contains("params="));
    }

    /// The XML catalog round-trips through quick-xml's Deserialize support.
    #[test]
    fn catalog_roundtrips_through_quick_xml_de() {
        let params = json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "File path" },
                "limit": { "type": "number" }
            },
            "required": ["path"]
        });
        let meta = create_list_available_tools(&[tool_with_params("sample_tool", params)]);
        let text = output_text(&meta, json!({}));
        // Parse the XML back to verify it's well-formed
        let parsed: ToolCatalog = quick_xml::de::from_str(&text).expect("parses");
        assert_eq!(parsed.tools.len(), 1);
        assert_eq!(parsed.tools[0].name, "sample_tool");
        assert!(parsed.tools[0].params.is_some());
    }
}
