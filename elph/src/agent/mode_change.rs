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
pub fn create_mode_change_tool(ui_tx: mpsc::UnboundedSender<AgentUiEvent>) -> AgentTool {
    elph_agent::simple_tool(
        Tool {
            name: "request_mode_change".into(),
            description: "Ask the user for permission to switch the agent mode. Use this when you need to execute commands or make file changes but the current mode restricts those actions.".into(),
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
