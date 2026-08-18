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
///
/// These tools are safe for Plan mode because they only read state or
/// interact with the user — they never mutate files, execute commands, or
/// make irreversible changes.
pub fn default_exploration_tools() -> Vec<String> {
    vec![
        "read_file".into(),
        "grep".into(),
        "find_path".into(),
        "list_dir".into(),
        "web_fetch".into(),
        "web_search".into(),
        "list_available_tools".into(),
        "ask_user_question".into(),
        "request_mode_change".into(),
    ]
}

fn default_mutating_tools() -> Vec<String> {
    vec![
        "write_file".into(),
        "edit_file".into(),
        "shell_exec".into(),
        "shell_use".into(),
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

/// Plan-mode tools for writing to `.elph/plans/*`.
///
/// Deprecated — system now handles plan file creation via `save_plan_to_disk`.
/// Kept as empty matcher so callers still compile; returns false for all tools
/// so they fall through to `is_mutating_tool` / `is_plan_mode_tool` filtering.
fn is_plan_file_tool(name: &str) -> bool {
    let _ = name;
    false
}

pub fn is_plan_mode_tool(name: &str, policy: Option<&ToolExposurePolicy>) -> bool {
    is_exploration_tool(name, policy) || is_goal_tool(name) || is_plan_file_tool(name) || is_read_only_mcp_tool(name)
}

/// Workspace mutating tools allowed in Plan (per-call approval). Excludes multi-agent tools.
pub fn is_plan_workspace_mutating_tool(name: &str, policy: Option<&ToolExposurePolicy>) -> bool {
    is_mutating_tool(name, policy) && !is_collaboration_tool(name, policy)
}

/// Tools the model may invoke while Plan is active (read-only plus approved workspace mutations).
pub fn is_plan_exposed_tool(name: &str, policy: Option<&ToolExposurePolicy>) -> bool {
    is_plan_mode_tool(name, policy) || is_plan_workspace_mutating_tool(name, policy)
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
            .filter(|name| is_plan_exposed_tool(name, policy))
            .cloned()
            .collect(),
    }
}

/// Whether a tool call is hard-blocked in Plan (collaboration / unlisted tools).
///
/// Workspace mutating tools stay available; the host must approve each call.
/// Implementing a plan still requires `<proposed_plan>` confirmation.
pub fn plan_mode_blocks_tool(mode: CollaborationMode, tool_name: &str, policy: Option<&ToolExposurePolicy>) -> bool {
    if mode != CollaborationMode::Plan {
        return false;
    }
    !is_plan_exposed_tool(tool_name, policy)
}

pub fn plan_mode_block_reason(tool_name: &str) -> String {
    format!(
        "Tool \"{tool_name}\" is not available in Plan mode. Use exploration or per-call \
         approved workspace tools to investigate, then wrap the implementation plan in \
         <proposed_plan>...</proposed_plan> for user confirmation."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_mode_exposes_workspace_mutating_tools() {
        let all = vec![
            "read_file".into(),
            "shell_exec".into(),
            "write_file".into(),
            "edit_file".into(),
            "create_dir".into(),
            "grep".into(),
            "spawn_agent".into(),
        ];
        let filtered = filter_active_tools(CollaborationMode::Plan, &all, None);
        assert!(filtered.contains(&"write_file".to_string()));
        assert!(filtered.contains(&"edit_file".to_string()));
        assert!(filtered.contains(&"create_dir".to_string()));
        assert!(filtered.contains(&"shell_exec".to_string()));
        assert!(filtered.contains(&"read_file".to_string()));
        assert!(filtered.contains(&"grep".to_string()));
        assert!(!filtered.contains(&"spawn_agent".to_string()));
    }

    #[test]
    fn plan_mode_includes_ask_user_question() {
        // ask_user_question is read-only — it only prompts the user, never mutates.
        assert!(is_plan_mode_tool("ask_user_question", None));
        assert!(!plan_mode_blocks_tool(CollaborationMode::Plan, "ask_user_question", None));
        let filtered = filter_active_tools(
            CollaborationMode::Plan,
            &["ask_user_question".into(), "write_file".into()],
            None,
        );
        assert!(filtered.contains(&"ask_user_question".to_string()));
        assert!(filtered.contains(&"write_file".to_string()));
    }

    #[test]
    fn plan_mode_includes_request_mode_change() {
        // request_mode_change is safe in Plan mode — it lets the agent escalate to
        // Build mode when code changes are needed.
        assert!(is_plan_mode_tool("request_mode_change", None));
        assert!(!plan_mode_blocks_tool(CollaborationMode::Plan, "request_mode_change", None));
        let filtered = filter_active_tools(
            CollaborationMode::Plan,
            &["request_mode_change".into(), "shell_exec".into()],
            None,
        );
        assert!(filtered.contains(&"request_mode_change".to_string()));
        assert!(filtered.contains(&"shell_exec".to_string()));
    }

    #[test]
    fn does_not_hard_block_workspace_mutating_in_plan_mode() {
        assert!(!plan_mode_blocks_tool(CollaborationMode::Plan, "shell_exec", None));
        assert!(!plan_mode_blocks_tool(CollaborationMode::Plan, "shell_use", None));
        assert!(!plan_mode_blocks_tool(CollaborationMode::Plan, "write_file", None));
        assert!(!plan_mode_blocks_tool(CollaborationMode::Plan, "edit_file", None));
        assert!(!plan_mode_blocks_tool(CollaborationMode::Plan, "create_dir", None));
        assert!(!plan_mode_blocks_tool(CollaborationMode::Default, "shell_exec", None));
    }

    #[test]
    fn plan_mode_hard_blocks_collaboration_tools() {
        assert!(plan_mode_blocks_tool(CollaborationMode::Plan, "spawn_agent", None));
        assert!(plan_mode_blocks_tool(CollaborationMode::Plan, "send_message", None));
        assert!(plan_mode_blocks_tool(CollaborationMode::Plan, "list_agents", None));
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
