//! Interactive user-prompting tools for Owly (`ask_text`, `ask_select`, `ask_confirm`).

mod bridge;
mod parse;

use std::sync::Arc;

use elph_agent::types::AgentToolResult;
use elph_agent::{AgentTool, simple_tool};
use elph_ai::Tool;
use serde_json::json;

pub use bridge::AskUserBridge;
pub use parse::format_args_summary;

/// All ask tool names (snake_case).
pub const ASK_TOOL_NAMES: &[&str] = &["ask_text", "ask_select", "ask_confirm"];

/// Register interactive ask tools bound to `bridge` (terminal dialoguer prompts).
pub fn create_ask_tools(bridge: AskUserBridge) -> Vec<AgentTool> {
    let bridge = Arc::new(bridge);
    vec![
        create_ask_text_tool(bridge.clone()),
        create_ask_select_tool(bridge.clone()),
        create_ask_confirm_tool(bridge),
    ]
}

pub fn create_ask_text_tool(bridge: Arc<AskUserBridge>) -> AgentTool {
    simple_tool(
        Tool {
            name: "ask_text".into(),
            description: "Ask the user a question and get a freeform text response. \
                          Use when you need clarification or additional information."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "question": {
                        "type": "string",
                        "description": "The question to ask the user"
                    },
                    "default": {
                        "type": "string",
                        "description": "Optional default when the user submits an empty answer"
                    }
                },
                "required": ["question"]
            }),
        },
        "Ask text input",
        move |id, args| {
            let bridge = bridge.clone();
            Box::pin(async move {
                let parsed = parse::parse_ask_text_args(&args).map_err(|e| anyhow::anyhow!("ask_text: {e}"))?;
                let answer = bridge
                    .prompt_text(&id, &parsed.question, parsed.default.as_deref())
                    .await?;
                Ok(AgentToolResult::text(answer))
            })
        },
    )
}

pub fn create_ask_select_tool(bridge: Arc<AskUserBridge>) -> AgentTool {
    simple_tool(
        Tool {
            name: "ask_select".into(),
            description: "Ask the user to pick one option from a list. \
                          Use for multiple-choice decisions — not for yes/no (use ask_confirm)."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "question": {
                        "type": "string",
                        "description": "The question to ask the user"
                    },
                    "options": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Non-empty choices to present"
                    },
                    "default": {
                        "type": "integer",
                        "description": "Optional 0-based index of the default selection"
                    }
                },
                "required": ["question", "options"]
            }),
        },
        "Ask multiple choice",
        move |id, args| {
            let bridge = bridge.clone();
            Box::pin(async move {
                let parsed = parse::parse_ask_select_args(&args).map_err(|e| anyhow::anyhow!("ask_select: {e}"))?;
                let answer = bridge
                    .prompt_select(&id, &parsed.question, &parsed.options, parsed.default_index)
                    .await?;
                Ok(AgentToolResult::text(answer))
            })
        },
    )
}

pub fn create_ask_confirm_tool(bridge: Arc<AskUserBridge>) -> AgentTool {
    simple_tool(
        Tool {
            name: "ask_confirm".into(),
            description: "Ask the user a yes/no confirmation. Returns `yes` or `no`.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "question": {
                        "type": "string",
                        "description": "The yes/no question to ask the user"
                    },
                    "default": {
                        "type": "boolean",
                        "description": "Default when the user presses Enter (true=yes, false=no)"
                    }
                },
                "required": ["question"]
            }),
        },
        "Ask confirmation",
        move |id, args| {
            let bridge = bridge.clone();
            Box::pin(async move {
                let parsed = parse::parse_ask_confirm_args(&args).map_err(|e| anyhow::anyhow!("ask_confirm: {e}"))?;
                let answer = bridge.prompt_confirm(&id, &parsed.question, parsed.default).await?;
                Ok(AgentToolResult::text(answer))
            })
        },
    )
}
