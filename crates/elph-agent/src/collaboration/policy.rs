//! Tool exposure policy per collaboration mode.

use super::CollaborationMode;

/// Built-in tools allowed while planning (read-only exploration).
const PLAN_MODE_TOOLS: &[&str] = &[
    "read_file",
    "grep",
    "find_path",
    "list_dir",
    "web_fetch",
    "web_search",
    "diagnostics",
    "ask_text",
    "ask_select",
    "ask_confirm",
];

/// Tools that mutate workspace state or spawn work — blocked in Plan mode.
const MUTATING_TOOLS: &[&str] = &[
    "write_file",
    "edit_file",
    "bash",
    "create_dir",
    "copy_path",
    "delete_path",
    "move_path",
    "spawn_agent",
    "send_message",
    "followup_task",
    "wait_agent",
];

/// Multi-agent tools — only in Default mode.
const MULTI_AGENT_TOOLS: &[&str] = &[
    "spawn_agent",
    "send_message",
    "followup_task",
    "wait_agent",
    "list_agents",
];

pub fn is_mcp_tool(name: &str) -> bool {
    name.starts_with("mcp_")
}

pub fn is_goal_tool(name: &str) -> bool {
    matches!(name, "create_goal" | "get_goal" | "update_goal" | "set_goal_budget")
}

pub fn is_plan_mode_tool(name: &str) -> bool {
    PLAN_MODE_TOOLS.contains(&name) || is_mcp_tool(name) || is_goal_tool(name)
}

pub fn is_mutating_tool(name: &str) -> bool {
    if MUTATING_TOOLS.contains(&name) {
        return true;
    }
    // MCP tools are treated as potentially mutating unless they are read-only bridge tools.
    // Product-level policy may refine this via `McpToolRegistry::tool_requires_approval`.
    if is_mcp_tool(name) {
        return !is_mcp_read_only_bridge_tool(name);
    }
    false
}

/// MCP bridge tools that only inspect server state (safe without approval by default).
pub fn is_mcp_read_only_bridge_tool(name: &str) -> bool {
    name.ends_with("__list_resources") || name.ends_with("__list_prompts") || name.ends_with("__read_resource")
}

pub fn is_multi_agent_tool(name: &str) -> bool {
    MULTI_AGENT_TOOLS.contains(&name)
}

/// Filter active tool names for the given collaboration mode.
pub fn filter_active_tools(mode: CollaborationMode, all_names: &[String]) -> Vec<String> {
    match mode {
        CollaborationMode::Default => all_names.to_vec(),
        CollaborationMode::Plan => all_names
            .iter()
            .filter(|name| is_plan_mode_tool(name))
            .cloned()
            .collect(),
    }
}

/// Whether a tool call should be blocked in Plan mode.
pub fn plan_mode_blocks_tool(mode: CollaborationMode, tool_name: &str) -> bool {
    mode == CollaborationMode::Plan && (is_mutating_tool(tool_name) || !is_plan_mode_tool(tool_name))
}

pub fn plan_mode_block_reason(tool_name: &str) -> String {
    format!(
        "Tool \"{tool_name}\" is not available in Plan mode. Use read-only tools to explore, \
         then wrap your implementation plan in <proposed_plan>...</proposed_plan> for user confirmation."
    )
}

pub use crate::prompt::builtin::plan::plan_mode_system_prompt;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_mode_filters_mutating_tools() {
        let all = vec![
            "read_file".into(),
            "bash".into(),
            "write_file".into(),
            "grep".into(),
        ];
        let filtered = filter_active_tools(CollaborationMode::Plan, &all);
        assert_eq!(filtered, vec!["read_file".to_string(), "grep".to_string()]);
    }

    #[test]
    fn blocks_bash_in_plan_mode() {
        assert!(plan_mode_blocks_tool(CollaborationMode::Plan, "bash"));
        assert!(!plan_mode_blocks_tool(CollaborationMode::Default, "bash"));
    }
}
