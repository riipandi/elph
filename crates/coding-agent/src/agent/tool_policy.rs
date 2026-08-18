//! Tool exposure and approval policy for TUI agent modes.

use std::collections::HashSet;
use std::sync::{Arc, OnceLock};

use tokio::sync::Mutex;

use elph_agent::{
    CollaborationMode, McpToolRegistry, ToolExposurePolicy, filter_active_tools, is_exploration_tool, is_mcp_tool,
    is_mutating_tool, is_plan_workspace_mutating_tool, is_read_only_mcp_tool,
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
            "ask_user_question".into(),
            "request_mode_change".into(),
            "list_available_tools".into(),
            // On-demand skill catalog (read-only listing; never drops a skill).
            "list_skills".into(),
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
    /// False for `elph run` — no TUI/ACP to answer approval prompts.
    interactive: bool,
    pub default_tools: Option<Vec<String>>,
}

impl AgentModePolicy {
    pub fn new(mode: AgentMode) -> Self {
        Self {
            mode,
            brave: mode == AgentMode::Brave,
            session_allowed: Mutex::new(HashSet::new()),
            session_allow_all: Mutex::new(false),
            mcp_registry: None,
            interactive: true,
            default_tools: None,
        }
    }

    pub fn with_default_tools(mut self, tools: Option<Vec<String>>) -> Self {
        self.default_tools = tools;
        self
    }

    pub fn set_interactive(&mut self, interactive: bool) {
        self.interactive = interactive;
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
    ///
    /// MCP tools (`mcp_*`) are **default-inactive**: registered on the harness but
    /// omitted from the active set until the model activates them via
    /// `list_available_tools(name_prefix: …)` → `added_tool_names` → harness lazy
    /// activation. Callers that must keep already-activated MCP tools across a
    /// reconcile should re-merge them (see `reconcile_harness_tools`).
    pub fn active_tool_names_for_mode(
        mode: AgentMode,
        all_registered: &[String],
        mcp_registry: Option<&McpToolRegistry>,
    ) -> Vec<String> {
        let policy = coding_tool_exposure_policy();
        // MCP schemas stay off the wire until explicitly activated (lazy load).
        let without_mcp: Vec<String> = all_registered
            .iter()
            .filter(|name| !is_mcp_tool(name))
            .cloned()
            .collect();
        let names = match mode {
            AgentMode::Build | AgentMode::Brave => without_mcp,
            AgentMode::Plan => filter_active_tools(CollaborationMode::Plan, &without_mcp, Some(policy)),
            AgentMode::Ask => filter_ask_mode_tools(&without_mcp, mcp_registry),
        };
        Self::ensure_list_available_tool(names)
    }

    /// Whether an MCP tool may remain/become active in this mode after lazy activation.
    pub fn mcp_allowed_in_mode(mode: AgentMode, name: &str, mcp_registry: Option<&McpToolRegistry>) -> bool {
        if !is_mcp_tool(name) {
            return false;
        }
        match mode {
            AgentMode::Build | AgentMode::Brave => true,
            AgentMode::Plan => is_read_only_mcp_tool(name),
            AgentMode::Ask => is_ask_mode_tool(name, mcp_registry, coding_tool_exposure_policy()),
        }
    }

    fn ensure_list_available_tool(mut names: Vec<String>) -> Vec<String> {
        if !names.iter().any(|n| n == "list_available_tools") {
            names.push("list_available_tools".into());
        }
        if !names.iter().any(|n| n == "list_skills") {
            names.push("list_skills".into());
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
            return is_plan_workspace_mutating_tool(tool_name, Some(coding_tool_exposure_policy()));
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
        if !self.interactive {
            return Err(format!(
                "Tool \"{tool_name}\" needs interactive approval. Headless Plan cannot grant \
                 mutating workspace tools; investigate with read-only tools or run Plan in the TUI."
            ));
        }
        let once_only = self.mode == AgentMode::Plan;
        if !once_only {
            if *self.session_allow_all.lock().await {
                return Ok(true);
            }
            if self.session_allowed.lock().await.contains(&tool_name) {
                return Ok(true);
            }
        }
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();
        let _ = ui_tx.send(AgentUiEvent::ToolApprovalRequired(ToolApprovalRequest {
            tool_call_id,
            tool_name: tool_name.clone(),
            args_summary,
            once_only,
            response_tx,
        }));
        match response_rx.await {
            Ok(ToolApprovalChoice::Approve) => Ok(true),
            Ok(ToolApprovalChoice::AllowSession) if !once_only => {
                self.session_allowed.lock().await.insert(tool_name);
                Ok(true)
            }
            Ok(ToolApprovalChoice::AllowAllTools) if !once_only => {
                *self.session_allow_all.lock().await = true;
                Ok(true)
            }
            Ok(ToolApprovalChoice::AllowSession | ToolApprovalChoice::AllowAllTools) => Ok(true),
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

pub fn from_agent_thinking(level: elph_agent::AgentThinkingLevel) -> crate::types::ThinkingLevel {
    use crate::types::ThinkingLevel;
    use elph_agent::AgentThinkingLevel;
    match level {
        AgentThinkingLevel::Off => ThinkingLevel::Off,
        AgentThinkingLevel::Minimal => ThinkingLevel::Minimal,
        AgentThinkingLevel::Low => ThinkingLevel::Low,
        AgentThinkingLevel::Medium => ThinkingLevel::Medium,
        AgentThinkingLevel::High => ThinkingLevel::High,
        AgentThinkingLevel::Xhigh => ThinkingLevel::Xhigh,
        AgentThinkingLevel::Max => ThinkingLevel::Max,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_mode_exposes_native_tools_but_not_mcp() {
        let all = vec![
            "read_file".into(),
            "write_file".into(),
            "shell_exec".into(),
            "mcp_deepwiki__ask_question".into(),
            "mcp_github__list_issues".into(),
        ];
        let active = AgentModePolicy::active_tool_names_for_mode(AgentMode::Build, &all, None);
        // Native + meta (`list_available_tools`, `list_skills`); MCP stays inactive.
        assert!(active.contains(&"write_file".to_string()));
        assert!(active.contains(&"list_available_tools".to_string()));
        assert!(active.contains(&"list_skills".to_string()));
        assert!(!active.iter().any(|n| n.starts_with("mcp_")));
        assert_eq!(active.len(), 5);
    }

    #[test]
    fn ask_and_plan_also_keep_mcp_inactive_by_default() {
        let all = vec![
            "read_file".into(),
            "write_file".into(),
            "mcp_wiki__read_wiki".into(),
            "mcp_fs__write_file".into(),
        ];
        for mode in [AgentMode::Ask, AgentMode::Plan] {
            let active = AgentModePolicy::active_tool_names_for_mode(mode, &all, None);
            assert!(
                !active.iter().any(|n| n.starts_with("mcp_")),
                "{mode:?} must not default-activate MCP tools: {active:?}"
            );
            assert!(active.contains(&"read_file".to_string()));
            assert!(active.contains(&"list_available_tools".to_string()));
        }
    }

    #[test]
    fn mcp_allowed_in_mode_gates_mutating_tools_for_plan_ask() {
        assert!(AgentModePolicy::mcp_allowed_in_mode(
            AgentMode::Build,
            "mcp_fs__write_file",
            None
        ));
        assert!(AgentModePolicy::mcp_allowed_in_mode(
            AgentMode::Plan,
            "mcp_wiki__read_wiki",
            None
        ));
        assert!(!AgentModePolicy::mcp_allowed_in_mode(
            AgentMode::Plan,
            "mcp_fs__write_file",
            None
        ));
        assert!(AgentModePolicy::mcp_allowed_in_mode(
            AgentMode::Ask,
            "mcp_wiki__list_pages",
            None
        ));
        assert!(!AgentModePolicy::mcp_allowed_in_mode(
            AgentMode::Ask,
            "mcp_fs__write_file",
            None
        ));
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
        assert!(!active.contains(&"write_file".to_string()));
    }

    #[test]
    fn plan_mode_allows_plan_file_tools() {
        let all = vec![
            "read_file".into(),
            "edit_file".into(),
            "write_file".into(),
            "create_dir".into(),
            "web_search".into(),
            "ask_user_question".into(),
            "request_mode_change".into(),
        ];
        let active = AgentModePolicy::active_tool_names_for_mode(AgentMode::Plan, &all, None);
        assert!(active.contains(&"read_file".to_string()));
        assert!(active.contains(&"web_search".to_string()));
        assert!(active.contains(&"edit_file".to_string()));
        assert!(active.contains(&"write_file".to_string()));
        assert!(active.contains(&"create_dir".to_string()));
        assert!(active.contains(&"ask_user_question".to_string()));
        assert!(active.contains(&"request_mode_change".to_string()));
    }

    #[test]
    fn plan_mode_requires_approval_for_mutating_tools() {
        let policy = AgentModePolicy::new(AgentMode::Plan);
        assert!(policy.needs_approval("write_file"));
        assert!(policy.needs_approval("shell_exec"));
        assert!(!policy.needs_approval("read_file"));
        assert!(!policy.needs_approval("grep"));
        assert!(!policy.needs_approval("mcp_wiki__read_wiki"));
        assert!(!policy.needs_approval("mcp_fs__write_file"));
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

    #[tokio::test]
    async fn plan_mode_always_prompts_even_after_allow_session_choice() {
        let policy = Arc::new(AgentModePolicy::new(AgentMode::Plan));
        let (ui_tx, mut ui_rx) = tokio::sync::mpsc::unbounded_channel();

        let policy_task = Arc::clone(&policy);
        let ui_tx_clone = ui_tx.clone();
        let approve = tokio::spawn(async move {
            policy_task
                .request_approval("c1".into(), "write_file".into(), "{}".into(), &ui_tx_clone)
                .await
        });
        let req = match ui_rx.recv().await {
            Some(AgentUiEvent::ToolApprovalRequired(req)) => req,
            other => panic!("expected ToolApprovalRequired, got {other:?}"),
        };
        assert!(req.once_only);
        let _ = req.response_tx.send(ToolApprovalChoice::AllowSession);
        assert_eq!(approve.await.expect("join"), Ok(true));

        let policy_task = Arc::clone(&policy);
        let ui_tx_clone = ui_tx.clone();
        let approve2 = tokio::spawn(async move {
            policy_task
                .request_approval("c2".into(), "write_file".into(), "{}".into(), &ui_tx_clone)
                .await
        });
        let req2 = match ui_rx.recv().await {
            Some(AgentUiEvent::ToolApprovalRequired(req)) => req,
            other => panic!("expected second ToolApprovalRequired, got {other:?}"),
        };
        assert!(req2.once_only);
        let _ = req2.response_tx.send(ToolApprovalChoice::Approve);
        assert_eq!(approve2.await.expect("join"), Ok(true));
    }

    #[tokio::test]
    async fn headless_plan_denies_mutating_tools_without_prompt() {
        let mut policy = AgentModePolicy::new(AgentMode::Plan);
        policy.set_interactive(false);
        let policy = Arc::new(policy);
        let (ui_tx, mut ui_rx) = tokio::sync::mpsc::unbounded_channel();
        let err = policy
            .request_approval("c1".into(), "write_file".into(), "{}".into(), &ui_tx)
            .await
            .expect_err("headless must not wait for approval");
        assert!(err.contains("interactive approval"), "{err}");
        assert!(ui_rx.try_recv().is_err(), "must not emit ToolApprovalRequired");
    }
}
