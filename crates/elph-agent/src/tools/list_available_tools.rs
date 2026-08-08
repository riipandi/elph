//! List available tools — meta tool that describes all tools the agent can use.
//!
//! The catalog is serialized to compact XML with `quick-xml` (serde `serialize`
//! feature): attributes carry the schema type / required / enum, element text
//! carries the description. XML is deliberately used over JSON — it is
//! token-cheaper and models parse it as easily as the `<available_skills>`
//! system-prompt block.

use elph_ai::Tool;

use serde::{Deserialize, Serialize};
use serde_json::json;
use serde_json::{Map, Value};

use crate::tools::simple_tool;
use crate::types::{AgentTool, AgentToolResult};

/// Serde model of `<available_tools>`; serialized via `quick_xml::se`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct ToolCatalog {
    #[serde(rename = "tool")]
    tools: Vec<ToolEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct ToolEntry {
    name: String,
    description: String,
    #[serde(rename = "parameters", skip_serializing_if = "Option::is_none")]
    parameters: Option<Parameters>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Parameters {
    #[serde(rename = "property")]
    properties: Vec<Property>,
}

/// One `<property>` element. Schema metadata lives in attributes; the description
/// is element text on leaf properties or a `<description>` child when the property
/// recurses into nested `<property>` elements (object-shaped schemas).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Property {
    #[serde(rename = "@name")]
    name: String,
    #[serde(rename = "@type")]
    type_name: String,
    #[serde(rename = "@enum", skip_serializing_if = "Option::is_none")]
    enum_values: Option<String>,
    #[serde(rename = "@required", skip_serializing_if = "is_false", default)]
    required: bool,
    #[serde(rename = "$text", skip_serializing_if = "Option::is_none")]
    text: Option<String>,
    #[serde(rename = "description", skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(rename = "property", skip_serializing_if = "Option::is_none")]
    children: Option<Vec<Property>>,
}

fn is_false(value: &bool) -> bool {
    !*value
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
    let values: Vec<String> = enum_values?.iter().filter_map(enum_scalar).collect();
    if values.is_empty() {
        None
    } else {
        Some(values.join("|"))
    }
}

/// Scalar enum members that fit inline in an XML attribute value. Objects, arrays,
/// and `null` members are skipped — they cannot be represented legibly as one token.
fn enum_scalar(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(xml_clean(s)),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

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

/// Names listed in the `required` array of an object schema.
fn required_names(schema: &Value) -> Vec<String> {
    schema
        .as_object()
        .and_then(|obj| obj.get("required"))
        .and_then(Value::as_array)
        .map(|arr| arr.iter().filter_map(Value::as_str).map(String::from).collect())
        .unwrap_or_default()
}

/// Render a single `<property>` from one JSON-Schema entry, recursing for
/// object-shaped schemas (direct `properties`, or arrays whose `items` is an object).
fn property_from_schema(name: &str, schema: &Value, required: bool) -> Property {
    let type_name = schema_type(schema);
    let enum_values = schema_enum(schema);
    let description = schema
        .get("description")
        .and_then(Value::as_str)
        .filter(|d| !d.is_empty())
        .map(xml_clean)
        .filter(|d| !d.is_empty());

    let mut children = None;
    if let Some(properties) = schema.get("properties").and_then(Value::as_object) {
        children = Some(render_properties(schema, properties));
    } else if schema.get("type").and_then(Value::as_str) == Some("array")
        && let Some(items) = schema.get("items")
        && let Some(properties) = items.get("properties").and_then(Value::as_object)
    {
        children = Some(render_properties(items, properties));
    }

    // Leaf properties carry the description as element text; object-shaped
    // properties carry it as a `<description>` child next to nested `<property>`s.
    let (text, child_description) = if children.is_some() {
        (None, description)
    } else {
        (description, None)
    };

    Property {
        name: xml_clean(name),
        type_name,
        enum_values,
        required,
        text,
        description: child_description,
        children,
    }
}

/// Render the `properties` of one object schema into `<property>` entries.
fn render_properties(object_schema: &Value, properties: &Map<String, Value>) -> Vec<Property> {
    let required = required_names(object_schema);
    properties
        .iter()
        .map(|(name, schema)| property_from_schema(name, schema, required.iter().any(|r| r == name)))
        .collect()
}

/// Build the `<available_tools>` XML catalog for a tool snapshot.
fn format_tool_catalog(tools: &[AgentTool]) -> String {
    let catalog = ToolCatalog {
        tools: tools
            .iter()
            .map(|tool| {
                let mut parameters = None;
                if let Some(params) = tool.tool.parameters.as_object()
                    && let Some(properties) = params.get("properties").and_then(Value::as_object)
                    && !properties.is_empty()
                {
                    parameters = Some(Parameters {
                        properties: render_properties(&tool.tool.parameters, properties),
                    });
                }
                ToolEntry {
                    name: xml_clean(&tool.tool.name),
                    description: xml_clean(&tool.tool.description),
                    parameters,
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
        assert!(text.starts_with("<available_tools>"), "{text}");
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
        // quick-xml's default `QuoteLevel::Minimal` escapes only what is required
        // in text content (`&`, `<`, `>`); quotes remain raw inside text nodes.
        assert!(text.contains("Use one of: \"a\", &lt;b&gt;, c &amp; d"));
    }

    #[test]
    fn tools_without_properties_omit_parameters() {
        let meta = create_list_available_tools(&[tool("bare")]);
        let text = output_text(&meta, json!({}));
        assert!(text.contains("<name>bare</name>"));
        assert!(!text.contains("<parameters>"));
    }

    /// The XML catalog round-trips through quick-xml's Deserialize support.
    #[test]
    fn catalog_roundtrips_through_quick_xml_de() {
        let params = json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "File path" },
                "limit": { "type": "number" },
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
        let tools = vec![
            tool("bare"),
            tool_with_params("sample_tool", params),
            tool("mcp_github__list_issues"),
        ];
        let meta = create_list_available_tools(&tools);
        let text = output_text(&meta, json!({}));
        let decoded: ToolCatalog = quick_xml::de::from_str(&text).expect("deserialize catalog");
        assert_eq!(decoded.tools.len(), 3);
        assert_eq!(decoded.tools[0].name, "bare");
        assert!(decoded.tools[0].parameters.is_none());
        let sample = decoded.tools[1].parameters.as_ref().expect("parameters");
        assert_eq!(sample.properties.len(), 3);
        assert_eq!(sample.properties[0].name, "path");
        assert_eq!(sample.properties[0].type_name, "string");
        assert!(sample.properties[0].required);
        assert_eq!(sample.properties[0].text.as_deref().unwrap(), "File path");
        assert_eq!(sample.properties[2].name, "ranges");
        let nested = sample.properties[2].children.as_ref().expect("nested children");
        assert_eq!(nested[0].name, "path");
        assert!(nested[0].required);
        assert_eq!(nested[0].type_name, "string");
    }

    /// Odd-but-legal schemas must never panic, must stay well-formed XML that
    /// round-trips through quick-xml's Deserialize, and must degrade gracefully.
    #[test]
    fn tolerates_exotic_schemas() {
        let params = json!({
            "type": "object",
            "properties": {
                // Numeric and boolean enum members are rendered inline.
                "level": { "type": "number", "enum": [1, 2, 3] },
                "flag": { "type": "boolean", "enum": [true, false] },
                // $ref-only schemas degrade to `any` instead of resolving.
                "user": { "$ref": "#/definitions/User" },
                // XML-special characters in attribute values are escaped.
                "we\"ird&name": { "type": "string", "description": "d" },
                // Tuple-form items become an untyped array.
                "t": { "type": "array", "items": [ { "type": "string" }, { "type": "number" } ] },
                // Control characters in descriptions are sanitized, not fatal.
                "x": { "type": "string", "description": "bad\u{0}desc" },
                // Deep nesting recurses without limitation.
                "a": {
                    "type": "object",
                    "properties": {
                        "b": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "c": {
                                        "type": "object",
                                        "properties": { "d": { "type": "string" } }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        });
        let meta = create_list_available_tools(&[tool_with_params("exotic", params)]);
        let text = output_text(&meta, json!({}));

        assert!(text.contains("<property name=\"level\" type=\"number\" enum=\"1|2|3\"/>"));
        assert!(text.contains("<property name=\"flag\" type=\"boolean\" enum=\"true|false\"/>"));
        assert!(text.contains("<property name=\"user\" type=\"any\"/>"));
        assert!(text.contains("<property name=\"we&quot;ird&amp;name\" type=\"string\">d</property>"));
        assert!(text.contains("<property name=\"t\" type=\"array\"/>"));
        // The NUL character must not make it into output (xml_clean strips it):
        // quick-xml passes control characters through raw, so we sanitize first.
        assert!(text.contains("<property name=\"x\" type=\"string\">baddesc</property>"));
        assert!(text.contains("type=\"object\"><property name=\"b\" type=\"array of object\">"));
        assert!(text.contains("<property name=\"c\" type=\"object\"><property name=\"d\" type=\"string\"/>"));

        // The whole catalog stays well-formed and structurally parseable.
        let decoded: ToolCatalog = quick_xml::de::from_str(&text).expect("deserialize catalog");
        let exotic = decoded.tools[0].parameters.as_ref().expect("parameters");
        let named = |name: &str| {
            exotic
                .properties
                .iter()
                .find(|p| p.name == name)
                .expect("property present")
        };
        assert_eq!(named("level").enum_values.as_deref(), Some("1|2|3"));
        assert_eq!(named("user").type_name, "any");
        assert_eq!(named("a").type_name, "object");
        let b = named("a").children.as_ref().expect("nested");
        assert_eq!(b[0].name, "b");
        assert_eq!(b[0].type_name, "array of object");
        let c = b[0].children.as_ref().expect("nested");
        assert_eq!(c[0].name, "c");
        assert_eq!(c[0].type_name, "object");
        let d = c[0].children.as_ref().expect("nested");
        assert_eq!(d[0].name, "d");
    }
}
