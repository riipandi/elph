//! Tool exposure and approval policy for TUI agent modes.

use std::collections::HashSet;
use std::sync::{Arc, OnceLock};

use tokio::sync::Mutex;

use elph_agent::{
    CollaborationMode, McpToolRegistry, ToolExposurePolicy, filter_active_tools, is_exploration_tool, is_mcp_tool,
    is_mutating_tool, is_read_only_mcp_tool,
};

use crate::types::AgentMode;

use super::events::AgentUiEvent;
use super::events::{ToolApprovalChoice, ToolApprovalRequest};

/// Exploration tools available to the Elph coding agent in Plan and Ask modes.
pub fn coding_tool_exposure_policy() -> &'static ToolExposurePolicy {
    static POLICY: OnceLock<ToolExposurePolicy> = OnceLock::new();
    POLICY.get_or_init(|| ToolExposurePolicy {
        exploration_tools: vec![
            "read_file".into(),
            "grep".into(),
            "find_path".into(),
            "list_dir".into(),
            "web_fetch".into(),
            "web_search".into(),
            "diagnostics".into(),
            "ask_user_question".into(),
            "list_available_tools".into(),
        ],
        ..ToolExposurePolicy::default()
    })
}

/// Filter tool names for Ask mode (read-only; optional MCP registry for approval hints).
pub fn filter_ask_mode_tools(all_names: &[String], mcp_registry: Option<&McpToolRegistry>) -> Vec<String> {
    let policy = coding_tool_exposure_policy();
    all_names
        .iter()
        .filter(|name| is_ask_mode_tool(name, mcp_registry, policy))
        .cloned()
        .collect()
}

fn is_ask_mode_tool(name: &str, mcp_registry: Option<&McpToolRegistry>, policy: &ToolExposurePolicy) -> bool {
    if is_exploration_tool(name, Some(policy)) {
        return true;
    }
    if matches!(name, "get_goal") {
        return true;
    }
    if is_read_only_mcp_tool(name) {
        return true;
    }
    if let Some(reg) = mcp_registry {
        return is_mcp_tool(name) && !reg.tool_requires_approval(name);
    }
    false
}

pub struct AgentModePolicy {
    pub mode: AgentMode,
    brave: bool,
    /// Tool names the user allowed for the rest of this session (per-tool).
    session_allowed: Mutex<HashSet<String>>,
    /// When true, skip approval for all tools until the process exits / policy resets.
    session_allow_all: Mutex<bool>,
    /// Optional MCP registry for fine-grained MCP tool approval.
    mcp_registry: Option<Arc<McpToolRegistry>>,
}

impl AgentModePolicy {
    pub fn new(mode: AgentMode) -> Self {
        Self {
            mode,
            brave: mode == AgentMode::Brave,
            session_allowed: Mutex::new(HashSet::new()),
            session_allow_all: Mutex::new(false),
            mcp_registry: None,
        }
    }

    pub fn with_mcp_registry(mut self, registry: Arc<McpToolRegistry>) -> Self {
        self.mcp_registry = Some(registry);
        self
    }

    pub fn set_mcp_registry(&mut self, registry: Arc<McpToolRegistry>) {
        self.mcp_registry = Some(registry);
    }

    pub fn set_mode(&mut self, mode: AgentMode) {
        self.mode = mode;
        self.brave = mode == AgentMode::Brave;
    }

    /// Resolve which registered tools are exposed to the model for `mode`.
    pub fn active_tool_names_for_mode(
        mode: AgentMode,
        all_registered: &[String],
        mcp_registry: Option<&McpToolRegistry>,
    ) -> Vec<String> {
        let policy = coding_tool_exposure_policy();
        let names = match mode {
            AgentMode::Build | AgentMode::Brave => all_registered.to_vec(),
            AgentMode::Plan => filter_active_tools(CollaborationMode::Plan, all_registered, Some(policy)),
            AgentMode::Ask => filter_ask_mode_tools(all_registered, mcp_registry),
        };
        Self::ensure_list_available_tool(names)
    }

    fn ensure_list_available_tool(mut names: Vec<String>) -> Vec<String> {
        if !names.iter().any(|n| n == "list_available_tools") {
            names.push("list_available_tools".into());
        }
        names.sort();
        names.dedup();
        names
    }

    pub fn needs_approval(&self, tool_name: &str) -> bool {
        if self.brave || self.mode == AgentMode::Ask {
            return false;
        }
        if self.mode == AgentMode::Plan {
            return false;
        }
        if is_mcp_tool(tool_name) {
            if let Some(reg) = &self.mcp_registry {
                return reg.tool_requires_approval(tool_name);
            }
            return is_mutating_tool(tool_name, None);
        }
        is_mutating_tool(tool_name, None)
    }

    pub async fn request_approval(
        &self,
        tool_call_id: String,
        tool_name: String,
        args_summary: String,
        ui_tx: &tokio::sync::mpsc::UnboundedSender<AgentUiEvent>,
    ) -> Result<bool, String> {
        if *self.session_allow_all.lock().await {
            return Ok(true);
        }
        if self.session_allowed.lock().await.contains(&tool_name) {
            return Ok(true);
        }
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();
        let _ = ui_tx.send(AgentUiEvent::ToolApprovalRequired(ToolApprovalRequest {
            tool_call_id,
            tool_name: tool_name.clone(),
            args_summary,
            response_tx,
        }));
        match response_rx.await {
            Ok(ToolApprovalChoice::Approve) => Ok(true),
            Ok(ToolApprovalChoice::AllowSession) => {
                self.session_allowed.lock().await.insert(tool_name);
                Ok(true)
            }
            Ok(ToolApprovalChoice::AllowAllTools) => {
                *self.session_allow_all.lock().await = true;
                Ok(true)
            }
            Ok(ToolApprovalChoice::Reject) => Ok(false),
            Err(_) => Err("Tool approval channel closed".into()),
        }
    }

    /// Whether the user chose "Allow all tools" for this session.
    #[cfg(test)]
    pub async fn session_allows_all_tools(&self) -> bool {
        *self.session_allow_all.lock().await
    }
}

pub fn agent_mode_from_setting(value: &str) -> AgentMode {
    match value.trim().to_ascii_lowercase().as_str() {
        "plan" => AgentMode::Plan,
        "ask" => AgentMode::Ask,
        "brave" => AgentMode::Brave,
        _ => AgentMode::Build,
    }
}

pub fn thinking_level_from_setting(value: &str) -> crate::types::ThinkingLevel {
    crate::types::ThinkingLevel::from_setting(value)
}

pub fn to_agent_thinking(level: crate::types::ThinkingLevel) -> elph_agent::AgentThinkingLevel {
    use crate::types::ThinkingLevel;
    use elph_agent::AgentThinkingLevel;
    match level {
        ThinkingLevel::Off => AgentThinkingLevel::Off,
        ThinkingLevel::Minimal => AgentThinkingLevel::Minimal,
        ThinkingLevel::Low => AgentThinkingLevel::Low,
        ThinkingLevel::Medium => AgentThinkingLevel::Medium,
        ThinkingLevel::High => AgentThinkingLevel::High,
        ThinkingLevel::Xhigh => AgentThinkingLevel::Xhigh,
        ThinkingLevel::Max => AgentThinkingLevel::Max,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_mode_exposes_all_registered_tools() {
        let all = vec!["read_file".into(), "write_file".into(), "shell_exec".into()];
        let active = AgentModePolicy::active_tool_names_for_mode(AgentMode::Build, &all, None);
        assert_eq!(active.len(), 4);
        assert!(active.contains(&"write_file".to_string()));
        assert!(active.contains(&"list_available_tools".to_string()));
    }

    #[test]
    fn ask_mode_hides_mutating_tools() {
        let all = vec![
            "read_file".into(),
            "write_file".into(),
            "web_search".into(),
            "create_dir".into(),
        ];
        let active = AgentModePolicy::active_tool_names_for_mode(AgentMode::Ask, &all, None);
        assert!(active.contains(&"read_file".to_string()));
        assert!(active.contains(&"web_search".to_string()));
        assert!(!active.contains(&"write_file".to_string()));
        assert!(!active.contains(&"create_dir".to_string()));
    }

    #[test]
    fn ask_mode_includes_coding_exploration_tools() {
        let mut all = coding_tool_exposure_policy().exploration_tools.clone();
        all.extend(["write_file".to_string(), "shell_exec".to_string()]);
        let active = AgentModePolicy::active_tool_names_for_mode(AgentMode::Ask, &all, None);
        assert!(active.contains(&"read_file".to_string()));
        assert!(active.contains(&"diagnostics".to_string()));
        assert!(!active.contains(&"write_file".to_string()));
    }

    #[test]
    fn plan_mode_hides_edit_tools() {
        let all = vec!["read_file".into(), "edit_file".into(), "web_search".into()];
        let active = AgentModePolicy::active_tool_names_for_mode(AgentMode::Plan, &all, None);
        assert!(active.contains(&"web_search".to_string()));
        assert!(!active.contains(&"edit_file".to_string()));
    }

    #[tokio::test]
    async fn allow_all_tools_skips_further_approval_prompts() {
        let policy = Arc::new(AgentModePolicy::new(AgentMode::Build));
        let (ui_tx, mut ui_rx) = tokio::sync::mpsc::unbounded_channel();

        // First call still prompts (we answer AllowAllTools).
        let policy_task = Arc::clone(&policy);
        let ui_tx_clone = ui_tx.clone();
        let approve = tokio::spawn(async move {
            policy_task
                .request_approval("c1".into(), "write_file".into(), r#"{"path":"a.rs"}"#.into(), &ui_tx_clone)
                .await
        });
        let req = match ui_rx.recv().await {
            Some(AgentUiEvent::ToolApprovalRequired(req)) => req,
            other => panic!("expected ToolApprovalRequired, got {other:?}"),
        };
        let _ = req.response_tx.send(ToolApprovalChoice::AllowAllTools);
        assert_eq!(approve.await.expect("join"), Ok(true));
        assert!(policy.session_allows_all_tools().await);

        // Second call for a different tool must not prompt.
        let ok = policy
            .request_approval("c2".into(), "shell_exec".into(), r#"{"command":"ls"}"#.into(), &ui_tx)
            .await
            .expect("approval");
        assert!(ok);
        assert!(ui_rx.try_recv().is_err(), "no second approval dialog expected");
    }

    #[tokio::test]
    async fn allow_session_only_skips_same_tool() {
        let policy = Arc::new(AgentModePolicy::new(AgentMode::Build));
        let (ui_tx, mut ui_rx) = tokio::sync::mpsc::unbounded_channel();

        let policy_task = Arc::clone(&policy);
        let ui_tx_clone = ui_tx.clone();
        let approve = tokio::spawn(async move {
            policy_task
                .request_approval("c1".into(), "write_file".into(), r#"{"path":"a.rs"}"#.into(), &ui_tx_clone)
                .await
        });
        let req = match ui_rx.recv().await {
            Some(AgentUiEvent::ToolApprovalRequired(req)) => req,
            other => panic!("expected ToolApprovalRequired, got {other:?}"),
        };
        let _ = req.response_tx.send(ToolApprovalChoice::AllowSession);
        assert_eq!(approve.await.expect("join"), Ok(true));

        // Same tool: no prompt.
        let ok = policy
            .request_approval("c2".into(), "write_file".into(), "{}".into(), &ui_tx)
            .await
            .expect("approval");
        assert!(ok);
        assert!(ui_rx.try_recv().is_err());

        // Different tool: still prompts.
        let policy_task = Arc::clone(&policy);
        let ui_tx_clone = ui_tx.clone();
        let approve2 = tokio::spawn(async move {
            policy_task
                .request_approval("c3".into(), "shell_exec".into(), "{}".into(), &ui_tx_clone)
                .await
        });
        let req2 = match ui_rx.recv().await {
            Some(AgentUiEvent::ToolApprovalRequired(req)) => req,
            other => panic!("expected second ToolApprovalRequired, got {other:?}"),
        };
        let _ = req2.response_tx.send(ToolApprovalChoice::Approve);
        assert_eq!(approve2.await.expect("join"), Ok(true));
    }
}
