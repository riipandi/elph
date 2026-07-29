//! Request-mode-change tool — lets the agent ask the user to switch agent modes.
//!
//! Available in Ask and Plan modes so the agent can escalate to Build or Brave
//! when it determines code changes or tool execution are needed.

use elph_agent::AgentTool;
use elph_ai::Tool;
use serde_json::json;
use tokio::sync::{mpsc, oneshot};

use super::events::{AgentUiEvent, ModeChangeRequest};

/// Create the `request_mode_change` tool.
///
/// `ui_tx` sends the mode-change prompt to the TUI.
///
/// The agent should call this when it needs to perform actions that require a different
/// mode. Important guidance:
/// - If already in **Build** mode, prefer asking for tool approval directly rather than
///   requesting a switch to Brave mode. Build mode with permission prompts is the safe
///   default — only request Brave for high-volume repetitive tasks where every tool call
///   would need approval.
/// - If in **Ask** or **Plan** mode, request a switch to **Build** mode (not Brave) to
///   keep safety guardrails in place. The agent can then ask for permission per-tool.
/// - Never request Brave for a simple edit or shell command — Build mode suffices.
pub fn create_mode_change_tool(ui_tx: mpsc::UnboundedSender<AgentUiEvent>) -> AgentTool {
    elph_agent::simple_tool(
        Tool {
            name: "request_mode_change".into(),
                constrained_sampling: None,
            description: "Ask the user for permission to switch the agent mode. Use this when you need to execute commands or make file changes but the current mode restricts those actions. \
                          /!\\ If you are already in Build mode, do NOT call this tool — just use the mutating tool directly and wait for the permission dialog. \
                          Only request Brave mode for high-volume repetitive tasks; for everything else, Build mode with per-tool approval is the correct choice."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "target_mode": {
                        "type": "string",
                        "enum": ["build", "brave"],
                        "description": "Target agent mode: build (tools require approval) or brave (all tools auto-approved)."
                    },
                    "reason": {
                        "type": "string",
                        "description": "Brief explanation why the mode change is needed (shown to the user)."
                    }
                },
                "required": ["target_mode", "reason"]
            }),
        },
        "request_mode_change",
        move |_, args| {
            let tx = ui_tx.clone();
            Box::pin(async move { execute_mode_change(tx, args).await })
        },
    )
}

async fn execute_mode_change(
    ui_tx: mpsc::UnboundedSender<AgentUiEvent>,
    args: serde_json::Value,
) -> anyhow::Result<elph_agent::AgentToolResult> {
    let target_mode = args
        .get("target_mode")
        .and_then(|v| v.as_str())
        .unwrap_or("build")
        .to_string();
    let reason = args.get("reason").and_then(|v| v.as_str()).unwrap_or("").to_string();

    let (response_tx, response_rx) = oneshot::channel();

    let request = ModeChangeRequest {
        target_mode,
        reason,
        response_tx,
    };

    ui_tx
        .send(AgentUiEvent::ModeChangeRequired(request))
        .map_err(|_| anyhow::anyhow!("UI channel closed"))?;

    let answer = response_rx
        .await
        .map_err(|_| anyhow::anyhow!("Mode change response channel closed"))?;

    if answer == "true" {
        Ok(elph_agent::AgentToolResult::text("ok"))
    } else {
        Ok(elph_agent::AgentToolResult::text("cancelled"))
    }
}
