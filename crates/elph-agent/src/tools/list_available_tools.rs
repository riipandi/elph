//! List available tools — meta tool that describes all tools the agent can use.

use elph_ai::Tool;

use serde_json::Value;
use serde_json::json;

use crate::tools::simple_tool;
use crate::types::{AgentTool, AgentToolResult};

/// Escape a string for XML text content and attribute values.
fn escape_xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// Compact JSON-Schema type description, e.g. `string`, `array of string`, `string|number`.
fn schema_type(schema: &Value) -> String {
    match schema.get("type").and_then(Value::as_str) {
        Some("array") => match schema.get("items") {
            Some(items) => {
                let inner = schema_type(items);
                if inner == "any" {
                    "array".to_string()
                } else {
                    format!("array of {inner}")
                }
            }
            None => "array".to_string(),
        },
        Some(other) => other.to_string(),
        None => {
            // Union types: join the branch types (e.g. `string|number`).
            for key in ["anyOf", "oneOf"] {
                if let Some(branches) = schema.get(key).and_then(Value::as_array) {
                    let types: Vec<String> = branches.iter().map(schema_type).collect();
                    if !types.is_empty() {
                        return types.join("|");
                    }
                }
            }
            "any".to_string()
        }
    }
}

/// Allowed values (`enum`) of a property, joined with `|` (uses the item enum for arrays).
fn schema_enum(schema: &Value) -> Option<String> {
    let enum_values = schema.get("enum").and_then(Value::as_array).or_else(|| {
        if schema.get("type").and_then(Value::as_str) == Some("array") {
            schema
                .get("items")
                .and_then(|items| items.get("enum").and_then(Value::as_array))
        } else {
            None
        }
    });
    let values: Vec<&str> = enum_values?.iter().filter_map(Value::as_str).collect();
    if values.is_empty() {
        None
    } else {
        Some(values.join("|"))
    }
}

/// Whether `name` appears in the `required` list of an object schema.
fn is_required(schema: &Value, name: &str) -> bool {
    is_required_in_list(schema.get("required").and_then(Value::as_array), name)
}

/// Whether `name` appears in an already-extracted `required` array.
fn is_required_in_list(required: Option<&Vec<Value>>, name: &str) -> bool {
    required.is_some_and(|list| list.iter().any(|v| v.as_str() == Some(name)))
}

/// Render one `<property>` element. Object-shaped schemas (direct `properties`, or an
/// array whose `items` is an object) recurse into their nested properties.
fn format_property(name: &str, schema: &Value, required: bool, indent: &str) -> String {
    let mut attrs = format!("name=\"{}\"", escape_xml(name));
    attrs.push_str(&format!(" type=\"{}\"", escape_xml(&schema_type(schema))));
    if let Some(enum_values) = schema_enum(schema) {
        attrs.push_str(&format!(" enum=\"{}\"", escape_xml(&enum_values)));
    }
    if required {
        attrs.push_str(" required=\"true\"");
    }

    let description = schema.get("description").and_then(Value::as_str);

    let mut nested = None;
    if let Some(properties) = schema.get("properties").and_then(Value::as_object) {
        nested = Some((schema, properties));
    } else if schema.get("type").and_then(Value::as_str) == Some("array")
        && let Some(items) = schema.get("items")
        && let Some(properties) = items.get("properties").and_then(Value::as_object)
    {
        nested = Some((items, properties));
    }

    if let Some((object_schema, properties)) = nested {
        let mut out = format!("{indent}<property {attrs}>\n");
        let child_indent = format!("{indent}  ");
        if let Some(desc) = description {
            out.push_str(&format!("{child_indent}<description>{}</description>\n", escape_xml(desc)));
        }
        for (prop_name, prop_schema) in properties {
            out.push_str(&format_property(
                prop_name,
                prop_schema,
                is_required(object_schema, prop_name),
                &child_indent,
            ));
        }
        out.push_str(&format!("{indent}</property>\n"));
        return out;
    }

    match description {
        Some(desc) => format!("{indent}<property {attrs}>{}</property>\n", escape_xml(desc)),
        None => format!("{indent}<property {attrs}/>\n"),
    }
}

/// Build the `<available_tools>` XML catalog for a tool snapshot.
///
/// XML is deliberately used over JSON: it is token-cheaper (short tags, attributes
/// carry the schema type/required/enum instead of a full JSON-Schema object) and
/// models parse it as easily as the `<available_skills>` system-prompt block.
fn format_tool_catalog(tools: &[AgentTool]) -> String {
    let mut out = String::from("<available_tools>\n");
    for tool in tools {
        out.push_str("  <tool>\n");
        out.push_str(&format!("    <name>{}</name>\n", escape_xml(&tool.tool.name)));
        out.push_str(&format!(
            "    <description>{}</description>\n",
            escape_xml(&tool.tool.description)
        ));
        if let Some(params) = tool.tool.parameters.as_object()
            && let Some(properties) = params.get("properties").and_then(Value::as_object)
            && !properties.is_empty()
        {
            let required = params.get("required").and_then(Value::as_array);
            out.push_str("    <parameters>\n");
            for (name, schema) in properties {
                out.push_str(&format_property(name, schema, is_required_in_list(required, name), "      "));
            }
            out.push_str("    </parameters>\n");
        }
        out.push_str("  </tool>\n");
    }
    out.push_str("</available_tools>");
    out
}

/// Create the `list_available_tools` tool from a snapshot of the current tool list.
///
/// The snapshot is captured at creation time. When MCP hot-reload changes the tool
/// set, the harness recreates tools via `set_tools`, which refreshes this snapshot.
///
/// The result is an XML catalog (one `<tool>` per entry) describing each tool's
/// name, description, and parameter schema as compact `<property>` elements.
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
        assert!(text.starts_with("<available_tools>\n"), "{text}");
        assert!(text.ends_with("</available_tools>"), "{text}");
        assert!(text.contains("<name>read_file</name>"));
        assert!(text.contains("<name>mcp_github__list_issues</name>"));
        assert!(text.contains("<name>mcp_browser__click</name>"));
        assert!(text.contains("<name>spawn_agent</name>"));
    }

    #[test]
    fn name_prefix_filters_mcp_server() {
        let meta = create_list_available_tools(&snapshot_tools());
        let result = output_result(&meta, json!({"name_prefix": "mcp_github__"}));
        let text = match result.content.first().expect("content") {
            crate::tools::types::ToolResultContent::Text(t) => t.text.clone(),
            _ => panic!("expected text"),
        };
        assert!(text.contains("<name>mcp_github__list_issues</name>"));
        assert!(text.contains("<name>mcp_github__get_issue</name>"));
        assert!(!text.contains("<name>read_file</name>"));
        assert!(!text.contains("<name>mcp_browser__navigate</name>"));
        assert!(!text.contains("<name>spawn_agent</name>"));
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
        assert!(text.contains("<name>read_file</name>"));
        assert!(text.contains("<name>mcp_browser__click</name>"));
        assert!(text.contains("<name>spawn_agent</name>"));
    }

    #[test]
    fn renders_property_schema_attributes() {
        let params = json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "File path" },
                "limit": { "type": "number" },
                "engine": {
                    "type": "string",
                    "enum": ["auto", "ddg", "exa"],
                    "description": "Search engine"
                },
                "extract": {
                    "type": "array",
                    "items": { "type": "string", "enum": ["links", "images", "text"] },
                    "description": "Which data to return"
                },
                "ranges": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "path": { "type": "string" },
                            "offset": { "type": "number" }
                        },
                        "required": ["path"]
                    },
                    "description": "Per-range settings"
                }
            },
            "required": ["path"]
        });
        let meta = create_list_available_tools(&[tool_with_params("sample_tool", params)]);
        let text = output_text(&meta, json!({}));
        assert!(text.contains("<property name=\"path\" type=\"string\" required=\"true\">File path</property>"));
        assert!(text.contains("<property name=\"limit\" type=\"number\"/>"));
        assert!(
            text.contains("<property name=\"engine\" type=\"string\" enum=\"auto|ddg|exa\">Search engine</property>")
        );
        assert!(text.contains("<property name=\"extract\" type=\"array of string\" enum=\"links|images|text\">Which data to return</property>"));
        // Nested object (array items) recursion, with its own `required` list.
        assert!(text.contains("<property name=\"ranges\" type=\"array of object\">"));
        assert!(text.contains("<description>Per-range settings</description>"));
        assert!(text.contains("<property name=\"path\" type=\"string\" required=\"true\"/>"));
        assert!(text.contains("<property name=\"offset\" type=\"number\"/>"));
    }

    #[test]
    fn escapes_xml_entities_in_text_and_attributes() {
        let params = json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Use one of: \"a\", <b>, c & d" }
            }
        });
        let meta = create_list_available_tools(&[tool_with_params("weird", params)]);
        let text = output_text(&meta, json!({}));
        assert!(text.contains("Use one of: &quot;a&quot;, &lt;b&gt;, c &amp; d"));
    }

    #[test]
    fn tools_without_properties_omit_parameters() {
        let meta = create_list_available_tools(&[tool("bare")]);
        let text = output_text(&meta, json!({}));
        assert!(text.contains("<name>bare</name>"));
        assert!(!text.contains("<parameters>"));
    }
}
