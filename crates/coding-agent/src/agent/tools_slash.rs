//! `/tools` slash command — list active tools without invoking the LLM.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use elph_agent::AgentTool;
use elph_agent::BuiltinToolsBuilder;
use elph_agent::runtime::LocalExecutionEnv;

use crate::types::AgentMode;

use super::CodingAgentSession;

/// Tool groups for readable `/tools` output (name → member tool ids).
const GROUPS: &[(&str, &[&str])] = &[
    ("Read & Search", &["read_file", "grep", "find_path", "list_dir"]),
    (
        "Edit",
        &[
            "edit_file",
            "write_file",
            "shell_exec",
            "shell_use",
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct ToolRow {
    name: String,
    group: String,
    description: String,
    server: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ToolsPayload {
    mode: String,
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

fn tools_payload(mode: AgentMode, tools: &[AgentTool], session_attached: bool) -> ToolsPayload {
    ToolsPayload {
        mode: mode.label().to_string(),
        count: tools.len(),
        session_attached,
        tools: collect_tool_rows(tools),
    }
}

fn session_note(session_attached: bool) -> Option<String> {
    if session_attached {
        None
    } else {
        Some("Note: Agent session unavailable — showing built-in tools only (no MCP).".to_string())
    }
}

fn format_header(payload: &ToolsPayload) -> String {
    format!("Available tools ({} mode, {} active)", payload.mode, payload.count)
}

/// Group tools by section with a tidy `name  description` column layout.
///
/// Output is plain text (no markdown) so it reads cleanly in the scrollable
/// `/tools` dialog. Each section is a header line; tool rows align their names
/// under a fixed-width first column and the description wraps after it.
fn format_tools_text(payload: &ToolsPayload, session_attached: bool) -> String {
    let mut lines = vec![format_header(payload)];
    let name_width = payload
        .tools
        .iter()
        .map(|row| row.name.chars().count())
        .max()
        .unwrap_or(0)
        .clamp(10, 28);
    let mut current_group: Option<&str> = None;
    let mut current_server: Option<&str> = None;
    for row in &payload.tools {
        if current_group != Some(row.group.as_str()) {
            lines.push(String::new());
            lines.push(row.group.clone());
            current_group = Some(&row.group);
            current_server = None;
        }
        if row.group == "MCP"
            && let Some(server) = row.server.as_deref()
            && current_server != Some(server)
        {
            lines.push(format!("  [{server}]"));
            current_server = Some(server);
        }
        lines.push(format!(
            "  {name:<width$}  {desc}",
            name = row.name,
            width = name_width,
            desc = row.description
        ));
    }
    if let Some(note) = session_note(session_attached) {
        lines.push(String::new());
        lines.push(note);
    }
    lines.join("\n")
}

pub fn format_tools_message(mode: AgentMode, tools: &[AgentTool], session_attached: bool) -> String {
    let payload = tools_payload(mode, tools, session_attached);
    format_tools_text(&payload, session_attached)
}

/// Built-in tool catalog when no agent session is attached.
pub fn format_builtin_tools_message() -> String {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let env = Arc::new(LocalExecutionEnv::new(&cwd));
    let tools = BuiltinToolsBuilder::all(env).build();
    format_tools_message(AgentMode::Build, &tools, false)
}

pub async fn active_tools_message(session: &CodingAgentSession) -> Result<String> {
    let mode = *session.mode_state().lock().await;
    let mut tools = session.harness().get_active_tools().await;
    tools.sort_by(|left, right| left.name().cmp(right.name()));
    Ok(format_tools_message(mode, &tools, true))
}

/// Full registry for ACP `/tools`: active tools plus lazy/inactive (MCP) names.
pub async fn discovery_tools_message(session: &CodingAgentSession) -> Result<String> {
    let mode = *session.mode_state().lock().await;
    let active: HashSet<String> = session
        .harness()
        .get_active_tools()
        .await
        .into_iter()
        .map(|tool| tool.name().to_string())
        .collect();
    let mut tools = session.harness().get_tools().await;
    tools.sort_by(|left, right| left.name().cmp(right.name()));
    let mut message = format_tools_message(mode, &tools, true);
    let inactive: Vec<String> = tools
        .iter()
        .map(|tool| tool.name().to_string())
        .filter(|name| !active.contains(name))
        .collect();
    if !inactive.is_empty() {
        message.push_str("\n\nInactive (lazy — call `list_available_tools` with name_prefix to activate):\n");
        for name in inactive {
            message.push_str(&format!("  {name}\n"));
        }
    }
    let skills = session.harness().get_resources().await.skills;
    if !skills.is_empty() {
        message.push_str("\nSkills (use `/skill:NAME` or `list_skills`):\n");
        for skill in skills {
            message.push_str(&format!("  {} — {}\n", skill.name, short_description(&skill.description)));
        }
    }
    Ok(message)
}

const TOOLS_SNAPSHOT_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(400);

/// Resolve `/tools` output for the TUI slash handler (sync).
///
/// While the agent is streaming, nested `try_block_on` on the TUI runtime can panic
/// or hang. Session tools are loaded on a **detached** thread with a short timeout;
/// on failure we fall back to the built-in catalog (with a note when a session exists).
pub fn tools_slash_message(session: Option<&Arc<CodingAgentSession>>) -> Result<String, String> {
    if let Some(session) = session {
        let session = Arc::clone(session);
        match elph_agent::runtime::try_block_on_detached(
            async move { active_tools_message(&session).await },
            TOOLS_SNAPSHOT_TIMEOUT,
        ) {
            Ok(Ok(message)) => return Ok(message),
            Ok(Err(err)) => {
                log::debug!("/tools session snapshot failed: {err:#}");
            }
            Err(err) => {
                log::debug!("/tools session snapshot unavailable: {err:#}");
            }
        }
        // Live session tools unavailable (busy/timeout/error) — built-in catalog + note.
        let mut message = format_builtin_tools_message();
        message.push_str(
            "\n\nNote: live session tools were unavailable (agent may be busy). Showing the built-in catalog.",
        );
        return Ok(message);
    }
    Ok(format_builtin_tools_message())
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
                constrained_sampling: None,
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
            sample_tool("shell_exec", "Execute shell commands."),
        ]
    }

    #[test]
    fn groups_known_tools_by_section() {
        let message = format_tools_message(AgentMode::Plan, &sample_tools(), true);
        assert!(message.contains("Available tools (Plan mode, 2 active)"));
        assert!(message.contains("Read & Search"));
        assert!(message.contains("read_file"));
        assert!(message.contains("Edit"));
        assert!(message.contains("shell_exec"));
        // Plain-text layout — no markdown table or bullet syntax.
        assert!(!message.contains("| Tool |"));
        assert!(!message.contains("- **`"));
    }

    #[test]
    fn builtin_fallback_notes_missing_session() {
        let message = format_builtin_tools_message();
        assert!(message.contains("Available tools"));
        assert!(message.contains("session unavailable"));
        assert!(!message.contains("| Tool |"));
    }

    #[test]
    fn slash_message_without_session_uses_builtin_catalog() {
        let message = tools_slash_message(None).expect("ok");
        assert!(message.contains("Available tools"));
        assert!(message.contains("session unavailable"));
    }
}
