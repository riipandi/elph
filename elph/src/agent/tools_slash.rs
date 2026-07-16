//! `/tools` slash command — list active tools without invoking the LLM.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use elph_agent::{AgentTool, BuiltinToolsBuilder, LocalExecutionEnv};
use serde::Serialize;

use crate::types::AgentMode;

use super::CodingAgentSession;

/// Tool groups for readable `/tools` output (name → member tool ids).
const GROUPS: &[(&str, &[&str])] = &[
    ("Read & Search", &["read_file", "grep", "find_path", "list_dir", "diagnostics"]),
    (
        "Edit",
        &[
            "edit_file",
            "write_file",
            "bash",
            "create_dir",
            "copy_path",
            "delete_path",
            "move_path",
        ],
    ),
    ("Web", &["web_search", "web_fetch"]),
    (
        "Collaboration",
        &[
            "ask_user_question",
            "spawn_agent",
            "send_message",
            "followup_task",
            "wait_agent",
            "list_agents",
        ],
    ),
    ("Goals", &["create_goal", "get_goal", "update_goal", "set_goal_budget"]),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolsOutputFormat {
    List,
    Json,
    Table,
}

impl ToolsOutputFormat {
    pub fn parse(args: &str) -> Result<Self, String> {
        match args.trim().to_ascii_lowercase().as_str() {
            "" | "table" => Ok(Self::Table),
            "list" => Ok(Self::List),
            "json" => Ok(Self::Json),
            other => Err(format!("unknown /tools format: {other} (use json, list, or table)")),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::List => "list",
            Self::Json => "json",
            Self::Table => "table",
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct ToolRow {
    name: String,
    group: String,
    description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    server: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct ToolsPayload {
    mode: String,
    format: String,
    count: usize,
    session_attached: bool,
    tools: Vec<ToolRow>,
}

fn short_description(description: &str) -> String {
    let first = description
        .split_once(". ")
        .map(|(sentence, _)| sentence)
        .unwrap_or(description);
    if first.chars().count() > 90 {
        let trimmed: String = first.chars().take(87).collect();
        format!("{trimmed}…")
    } else {
        first.to_string()
    }
}

fn tool_description_map(tools: &[AgentTool]) -> HashMap<String, String> {
    tools
        .iter()
        .map(|tool| (tool.name().to_string(), tool.tool.description.clone()))
        .collect()
}

fn mcp_server_name(tool_name: &str) -> Option<String> {
    tool_name
        .strip_prefix("mcp_")
        .and_then(|rest| rest.split_once("__"))
        .map(|(server, _)| server.to_string())
}

fn collect_tool_rows(tools: &[AgentTool]) -> Vec<ToolRow> {
    let descriptions = tool_description_map(tools);
    let mut listed = HashSet::new();
    let mut rows = Vec::new();

    for (group_name, expected) in GROUPS {
        for name in expected.iter().copied().filter(|name| descriptions.contains_key(*name)) {
            listed.insert(name.to_string());
            rows.push(ToolRow {
                name: name.to_string(),
                group: group_name.to_string(),
                description: short_description(descriptions.get(name).map(String::as_str).unwrap_or("")),
                server: None,
            });
        }
    }

    let mut mcp_by_server: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for name in descriptions.keys() {
        if !name.starts_with("mcp_") {
            continue;
        }
        let server = mcp_server_name(name).unwrap_or_else(|| "unknown".to_string());
        mcp_by_server.entry(server).or_default().push(name.clone());
    }

    for (server, mut names) in mcp_by_server {
        names.sort();
        for name in names {
            listed.insert(name.clone());
            let desc = descriptions.get(&name).map(String::as_str).unwrap_or("");
            let short_name = name.strip_prefix("mcp_").unwrap_or(&name);
            rows.push(ToolRow {
                name: short_name.to_string(),
                group: "MCP".to_string(),
                description: short_description(desc),
                server: Some(server.clone()),
            });
        }
    }

    let mut other: Vec<String> = descriptions
        .keys()
        .filter(|name| {
            name.as_str() != "list_available_tools" && !listed.contains(name.as_str()) && !name.starts_with("mcp_")
        })
        .cloned()
        .collect();
    other.sort();
    for name in other {
        let desc = descriptions.get(&name).map(String::as_str).unwrap_or("");
        rows.push(ToolRow {
            name: name.clone(),
            group: "Other".to_string(),
            description: short_description(desc),
            server: None,
        });
    }

    if descriptions.contains_key("list_available_tools") {
        rows.push(ToolRow {
            name: "list_available_tools".to_string(),
            group: "Meta".to_string(),
            description: "Lists tools via the agent (LLM tool)".to_string(),
            server: None,
        });
    }

    rows
}

fn tools_payload(
    mode: AgentMode,
    tools: &[AgentTool],
    session_attached: bool,
    format: ToolsOutputFormat,
) -> ToolsPayload {
    ToolsPayload {
        mode: mode.label().to_string(),
        format: format.label().to_string(),
        count: tools.len(),
        session_attached,
        tools: collect_tool_rows(tools),
    }
}

fn session_note(session_attached: bool) -> Option<String> {
    if session_attached {
        None
    } else {
        Some("> **Note:** Agent session unavailable — showing built-in tools only (no MCP).".to_string())
    }
}

fn format_header(payload: &ToolsPayload) -> String {
    format!("## Available tools ({} mode, {} active)", payload.mode, payload.count)
}

fn format_tool_bullet(row: &ToolRow) -> String {
    format!("- **`{}`** — {}", row.name, row.description)
}

fn format_tools_list(payload: &ToolsPayload, session_attached: bool) -> String {
    let mut lines = vec![format_header(payload)];
    let mut current_group: Option<&str> = None;
    let mut current_server: Option<&str> = None;

    for row in &payload.tools {
        if current_group != Some(row.group.as_str()) {
            lines.push(String::new());
            lines.push(format!("### {}", row.group));
            current_group = Some(&row.group);
            current_server = None;
        }
        if row.group == "MCP"
            && let Some(server) = row.server.as_deref()
            && current_server != Some(server)
        {
            lines.push(format!("**Server:** `{server}`"));
            current_server = Some(server);
        }
        lines.push(format_tool_bullet(row));
    }

    if let Some(note) = session_note(session_attached) {
        lines.push(String::new());
        lines.push(note);
    }

    lines.join("\n")
}

fn escape_table_cell(text: &str) -> String {
    text.replace('|', "\\|").replace('\n', " ")
}

fn format_tools_table(payload: &ToolsPayload, session_attached: bool) -> String {
    let mut lines = vec![
        format_header(payload),
        String::new(),
        "| Tool | Group | Description |".to_string(),
        "| --- | --- | --- |".to_string(),
    ];

    for row in &payload.tools {
        lines.push(format!(
            "| `{}` | {} | {} |",
            escape_table_cell(&row.name),
            escape_table_cell(&row.group),
            escape_table_cell(&row.description),
        ));
    }

    if let Some(note) = session_note(session_attached) {
        lines.push(String::new());
        lines.push(note);
    }

    lines.join("\n")
}

fn format_tools_json(payload: &ToolsPayload, session_attached: bool) -> String {
    let json = serde_json::to_string_pretty(payload).unwrap_or_else(|_| "{}".to_string());
    let mut lines = vec![format_header(payload), String::new(), format!("```json\n{json}\n```")];
    if let Some(note) = session_note(session_attached) {
        lines.push(String::new());
        lines.push(note);
    }
    lines.join("\n")
}

pub fn format_tools_message(
    mode: AgentMode,
    tools: &[AgentTool],
    session_attached: bool,
    format: ToolsOutputFormat,
) -> String {
    let payload = tools_payload(mode, tools, session_attached, format);
    match format {
        ToolsOutputFormat::List => format_tools_list(&payload, session_attached),
        ToolsOutputFormat::Table => format_tools_table(&payload, session_attached),
        ToolsOutputFormat::Json => format_tools_json(&payload, session_attached),
    }
}

/// Built-in tool catalog when no agent session is attached.
pub fn format_builtin_tools_message(format: ToolsOutputFormat) -> String {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let env = Arc::new(LocalExecutionEnv::new(&cwd));
    let tools = BuiltinToolsBuilder::all(env).build();
    format_tools_message(AgentMode::Build, &tools, false, format)
}

pub async fn active_tools_message(session: &CodingAgentSession, format: ToolsOutputFormat) -> Result<String> {
    let mode = *session.mode_state().lock().await;
    let mut tools = session.harness().get_active_tools().await;
    tools.sort_by(|left, right| left.name().cmp(right.name()));
    Ok(format_tools_message(mode, &tools, true, format))
}

/// Resolve `/tools` output for the TUI slash handler (sync).
pub fn tools_slash_message(session: Option<&CodingAgentSession>, args: &str) -> Result<String, String> {
    let format = ToolsOutputFormat::parse(args)?;
    if let Some(session) = session
        && let Ok(Ok(message)) = elph_agent::try_block_on(active_tools_message(session, format))
    {
        return Ok(message);
    }
    Ok(format_builtin_tools_message(format))
}

#[cfg(test)]
mod tests {
    use super::*;
    use elph_agent::{AgentToolResult, simple_tool};
    use elph_ai::Tool;

    fn sample_tool(name: &str, description: &str) -> AgentTool {
        simple_tool(
            Tool {
                name: name.into(),
                description: description.into(),
                parameters: serde_json::json!({"type": "object", "properties": {}}),
            },
            name,
            |_, _| Box::pin(async { Ok(AgentToolResult::text("ok")) }),
        )
    }

    fn sample_tools() -> Vec<AgentTool> {
        vec![
            sample_tool("read_file", "Read file contents from disk."),
            sample_tool("bash", "Execute shell commands."),
        ]
    }

    #[test]
    fn parse_tools_output_formats() {
        assert_eq!(ToolsOutputFormat::parse("").unwrap(), ToolsOutputFormat::Table);
        assert_eq!(ToolsOutputFormat::parse("table").unwrap(), ToolsOutputFormat::Table);
        assert_eq!(ToolsOutputFormat::parse("list").unwrap(), ToolsOutputFormat::List);
        assert_eq!(ToolsOutputFormat::parse("json").unwrap(), ToolsOutputFormat::Json);
        assert_eq!(ToolsOutputFormat::parse("TABLE").unwrap(), ToolsOutputFormat::Table);
        assert!(ToolsOutputFormat::parse("yaml").is_err());
    }

    #[test]
    fn format_groups_known_tools_as_markdown_list() {
        let message = format_tools_message(AgentMode::Plan, &sample_tools(), true, ToolsOutputFormat::List);
        assert!(message.contains("## Available tools (Plan mode, 2 active)"));
        assert!(message.contains("### Read & Search"));
        assert!(message.contains("**`read_file`**"));
        assert!(message.contains("### Edit"));
        assert!(message.contains("**`bash`**"));
    }

    #[test]
    fn format_tools_json_wraps_pretty_payload() {
        let message = format_tools_message(AgentMode::Plan, &sample_tools(), true, ToolsOutputFormat::Json);
        assert!(message.contains("```json"));
        assert!(message.contains("\"name\": \"read_file\""));
        assert!(message.contains("\"format\": \"json\""));
    }

    #[test]
    fn format_tools_table_renders_markdown_table() {
        let message = format_tools_message(AgentMode::Plan, &sample_tools(), true, ToolsOutputFormat::Table);
        assert!(message.contains("| Tool | Group | Description |"));
        assert!(message.contains("| `read_file` | Read & Search |"));
        assert!(message.contains("| `bash` | Edit |"));
    }

    #[test]
    fn format_builtin_fallback_notes_missing_session() {
        let message = format_builtin_tools_message(ToolsOutputFormat::Table);
        assert!(message.contains("## Available tools"));
        assert!(message.contains("| Tool | Group | Description |"));
        assert!(message.contains("Agent session unavailable"));
    }

    #[test]
    fn tools_slash_message_rejects_unknown_format() {
        assert!(tools_slash_message(None, "yaml").is_err());
    }
}
