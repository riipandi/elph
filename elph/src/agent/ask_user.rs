//! Ask-user tool — elph coding agent specific.
//!
//! Consolidates `ask_text`, `ask_select`, `ask_confirm` into a single
//! `ask_user_question` tool. The mode is detected from parameters:
//! - `options` present → select mode
//! - `default` is boolean → confirm mode
//! - otherwise → text input mode

use elph_agent::AgentTool;
use elph_ai::Tool;
use serde_json::{Value, json};
use tokio::sync::{mpsc, oneshot};

use super::events::{AgentUiEvent, UserQuestionOption, UserQuestionRequest};

/// Create the `ask_user_question` tool.
///
/// `ui_tx` is the channel used to present the question to the TUI and await a response.
pub fn create_ask_user_tool(ui_tx: mpsc::UnboundedSender<AgentUiEvent>) -> AgentTool {
    let tx = ui_tx;
    elph_agent::simple_tool(
        Tool {
            name: "ask_user_question".into(),
            description: "Ask the user a question to gather structured input, then returns the user's response. It can be a single question or a structured input request.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "question": {
                        "type": "string",
                        "description": "The question text to present to the user"
                    },
                    "options": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "value": { "type": "string" },
                                "label": { "type": "string" }
                            },
                            "required": ["value", "label"]
                        },
                        "description": "Optional list of choices for select mode"
                    },
                    "default": {
                        "description": "Optional default value (boolean for confirm, string for text/select)"
                    }
                },
                "required": ["question"]
            }),
        },
        "ask_user_question",
        move |_, args| {
            let tx = tx.clone();
            Box::pin(async move { execute_ask_user(tx, args).await })
        },
    )
}

async fn execute_ask_user(
    ui_tx: mpsc::UnboundedSender<AgentUiEvent>,
    args: Value,
) -> anyhow::Result<elph_agent::AgentToolResult> {
    let question = args
        .get("question")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing required argument: question"))?
        .to_string();

    let options = args.get("options").and_then(|v| v.as_array()).map(|arr| {
        arr.iter()
            .filter_map(|item| {
                let value = item.get("value")?.as_str()?.to_string();
                let label = item.get("label").and_then(|v| v.as_str()).unwrap_or(&value).to_string();
                Some(UserQuestionOption { value, label })
            })
            .collect::<Vec<_>>()
    });

    let default = args.get("default").map(|v| {
        if let Some(b) = v.as_bool() {
            b.to_string()
        } else {
            v.to_string().trim_matches('"').to_string()
        }
    });

    let (response_tx, response_rx) = oneshot::channel();

    let request = UserQuestionRequest {
        question,
        options,
        default,
        response_tx,
    };

    ui_tx
        .send(AgentUiEvent::UserQuestionRequired(request))
        .map_err(|_| anyhow::anyhow!("UI channel closed"))?;

    let answer = response_rx
        .await
        .map_err(|_| anyhow::anyhow!("User question response channel closed"))?;

    Ok(elph_agent::AgentToolResult::text(answer))
}
