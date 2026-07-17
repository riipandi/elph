//! Tool exposure policy per collaboration mode.

use std::sync::OnceLock;

use super::CollaborationMode;

/// Host-configurable tool exposure lists.
#[derive(Debug, Clone)]
pub struct ToolExposurePolicy {
    pub exploration_tools: Vec<String>,
    pub mutating_tools: Vec<String>,
    pub collaboration_tools: Vec<String>,
}

/// Default exploration tools for generic Plan mode (read-only builtins).
pub fn default_exploration_tools() -> Vec<String> {
    vec![
        "read_file".into(),
        "grep".into(),
        "find_path".into(),
        "list_dir".into(),
        "web_fetch".into(),
        "web_search".into(),
        "list_available_tools".into(),
    ]
}

fn default_mutating_tools() -> Vec<String> {
    vec![
        "write_file".into(),
        "edit_file".into(),
        "shell_exec".into(),
        "create_dir".into(),
        "copy_path".into(),
        "delete_path".into(),
        "move_path".into(),
        "spawn_agent".into(),
        "send_message".into(),
        "followup_task".into(),
        "wait_agent".into(),
    ]
}

fn default_collaboration_tools() -> Vec<String> {
    vec![
        "spawn_agent".into(),
        "send_message".into(),
        "followup_task".into(),
        "wait_agent".into(),
        "list_agents".into(),
    ]
}

impl Default for ToolExposurePolicy {
    fn default() -> Self {
        Self {
            exploration_tools: default_exploration_tools(),
            mutating_tools: default_mutating_tools(),
            collaboration_tools: default_collaboration_tools(),
        }
    }
}

fn runtime_default_policy() -> &'static ToolExposurePolicy {
    static POLICY: OnceLock<ToolExposurePolicy> = OnceLock::new();
    POLICY.get_or_init(ToolExposurePolicy::default)
}

fn active_policy(policy: Option<&ToolExposurePolicy>) -> &ToolExposurePolicy {
    match policy {
        Some(policy) => policy,
        None => runtime_default_policy(),
    }
}

pub fn is_mcp_tool(name: &str) -> bool {
    name.starts_with("mcp_")
}

pub fn is_goal_tool(name: &str) -> bool {
    matches!(name, "create_goal" | "get_goal" | "update_goal" | "set_goal_budget")
}

pub fn is_exploration_tool(name: &str, policy: Option<&ToolExposurePolicy>) -> bool {
    active_policy(policy).exploration_tools.iter().any(|tool| tool == name)
}

/// MCP tools that only read or list remote state (safe in Plan / Ask).
pub fn is_read_only_mcp_tool(name: &str) -> bool {
    if !is_mcp_tool(name) {
        return false;
    }
    if is_mcp_read_only_bridge_tool(name) {
        return true;
    }
    let lower = name.to_ascii_lowercase();
    lower.contains("__read")
        || lower.contains("__list")
        || lower.contains("__get")
        || lower.contains("__search")
        || lower.contains("__fetch")
        || lower.contains("__browse")
        || lower.ends_with("_read")
}

pub fn is_plan_mode_tool(name: &str, policy: Option<&ToolExposurePolicy>) -> bool {
    is_exploration_tool(name, policy) || is_goal_tool(name) || is_read_only_mcp_tool(name)
}

pub fn is_mutating_tool(name: &str, policy: Option<&ToolExposurePolicy>) -> bool {
    if active_policy(policy).mutating_tools.iter().any(|tool| tool == name) {
        return true;
    }
    if is_mcp_tool(name) {
        return !is_mcp_read_only_bridge_tool(name);
    }
    false
}

/// MCP bridge tools that only inspect server state (safe without approval by default).
pub fn is_mcp_read_only_bridge_tool(name: &str) -> bool {
    name.ends_with("__list_resources") || name.ends_with("__list_prompts") || name.ends_with("__read_resource")
}

pub fn is_collaboration_tool(name: &str, policy: Option<&ToolExposurePolicy>) -> bool {
    active_policy(policy)
        .collaboration_tools
        .iter()
        .any(|tool| tool == name)
}

/// Filter active tool names for the given collaboration mode.
pub fn filter_active_tools(
    mode: CollaborationMode,
    all_names: &[String],
    policy: Option<&ToolExposurePolicy>,
) -> Vec<String> {
    match mode {
        CollaborationMode::Default => all_names.to_vec(),
        CollaborationMode::Plan => all_names
            .iter()
            .filter(|name| is_plan_mode_tool(name, policy))
            .cloned()
            .collect(),
    }
}

/// Whether a tool call should be blocked in Plan mode.
pub fn plan_mode_blocks_tool(mode: CollaborationMode, tool_name: &str, policy: Option<&ToolExposurePolicy>) -> bool {
    mode == CollaborationMode::Plan && (is_mutating_tool(tool_name, policy) || !is_plan_mode_tool(tool_name, policy))
}

pub fn plan_mode_block_reason(tool_name: &str) -> String {
    format!(
        "Tool \"{tool_name}\" is not available in Plan mode. Use read-only tools to explore, \
         then wrap your implementation plan in <proposed_plan>...</proposed_plan> for user confirmation."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_mode_filters_mutating_tools() {
        let all = vec![
            "read_file".into(),
            "shell_exec".into(),
            "write_file".into(),
            "grep".into(),
        ];
        let filtered = filter_active_tools(CollaborationMode::Plan, &all, None);
        assert_eq!(filtered, vec!["read_file".to_string(), "grep".to_string()]);
    }

    #[test]
    fn blocks_shell_exec_in_plan_mode() {
        assert!(plan_mode_blocks_tool(CollaborationMode::Plan, "shell_exec", None));
        assert!(!plan_mode_blocks_tool(CollaborationMode::Default, "shell_exec", None));
    }

    #[test]
    fn plan_mode_includes_list_available_tools() {
        assert!(is_plan_mode_tool("list_available_tools", None));
    }

    #[test]
    fn plan_mode_excludes_mutating_mcp_by_default() {
        assert!(!is_plan_mode_tool("mcp_fs__write_file", None));
        assert!(is_plan_mode_tool("mcp_wiki__read_wiki", None));
    }
}
