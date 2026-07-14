//! Tool exposure and approval policy for TUI agent modes.

use crate::types::AgentMode;
use elph_agent::{McpToolRegistry, is_mcp_tool, is_mutating_tool};
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::Mutex;

use super::events::AgentUiEvent;
use super::events::{ToolApprovalChoice, ToolApprovalRequest};

const READ_ONLY_TOOLS: &[&str] = &[
    "read_file",
    "grep",
    "find_path",
    "list_dir",
    "web_fetch",
    "web_search",
    "diagnostics",
];

pub struct AgentModePolicy {
    pub mode: AgentMode,
    brave: bool,
    session_allowed: Mutex<HashSet<String>>,
    /// Optional MCP registry for fine-grained MCP tool approval.
    mcp_registry: Option<Arc<McpToolRegistry>>,
}

impl AgentModePolicy {
    pub fn new(mode: AgentMode) -> Self {
        Self {
            mode,
            brave: mode == AgentMode::Brave,
            session_allowed: Mutex::new(HashSet::new()),
            mcp_registry: None,
        }
    }

    pub fn with_mcp_registry(mut self, registry: Arc<McpToolRegistry>) -> Self {
        self.mcp_registry = Some(registry);
        self
    }

    pub fn set_mode(&mut self, mode: AgentMode) {
        self.mode = mode;
        self.brave = mode == AgentMode::Brave;
    }

    pub fn read_only_tool_names() -> Vec<String> {
        READ_ONLY_TOOLS.iter().map(|s| (*s).to_string()).collect()
    }

    pub fn needs_approval(&self, tool_name: &str) -> bool {
        if self.brave || self.mode == AgentMode::Ask {
            return false;
        }
        if is_mcp_tool(tool_name) {
            if let Some(reg) = &self.mcp_registry {
                return reg.tool_requires_approval(tool_name);
            }
            return is_mutating_tool(tool_name);
        }
        is_mutating_tool(tool_name)
    }

    pub async fn request_approval(
        &self,
        tool_call_id: String,
        tool_name: String,
        args_summary: String,
        ui_tx: &tokio::sync::mpsc::UnboundedSender<AgentUiEvent>,
    ) -> Result<bool, String> {
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
            Ok(ToolApprovalChoice::Reject) => Ok(false),
            Err(_) => Err("Tool approval channel closed".into()),
        }
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
    }
}
