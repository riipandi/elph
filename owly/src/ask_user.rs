//! Interactive user-prompting tools for Owly.
//!
//! These tools allow the AI agent to ask the user questions
//! (text input, selection) during a conversation.

use elph_agent::types::AgentToolResult;
use elph_agent::{AgentTool, simple_tool};
use elph_ai::Tool;
use serde_json::json;

/// Create the `ask_text` tool that prompts the user for freeform text input.
pub fn create_ask_text_tool() -> AgentTool {
    simple_tool(
        Tool {
            name: "ask_text".into(),
            description: "Ask the user a question and get a freeform text response. \
                          Use this when you need clarification, confirmation, \
                          or additional information from the user."
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
                        "description": "Optional default value if the user presses enter"
                    }
                },
                "required": ["question"]
            }),
        },
        "Ask text input",
        |_, args| {
            Box::pin(async move {
                let question = args
                    .get("question")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Please answer:");
                let default = args.get("default").and_then(|v| v.as_str());

                let result = tokio::task::spawn_blocking({
                    let question = question.to_string();
                    let default = default.map(|s| s.to_string());
                    move || {
                        use dialoguer::Input;
                        let mut input = Input::<String>::new().with_prompt(&question).allow_empty(true);
                        if let Some(ref d) = default {
                            input = input.default(d.clone());
                        }
                        input.interact_text()
                    }
                })
                .await
                .map_err(|e| anyhow::anyhow!("ask_text interrupted: {e}"))?
                .map_err(|e| anyhow::anyhow!("ask_text failed: {e}"))?;

                Ok(AgentToolResult::text(result))
            })
        },
    )
}

/// Create the `ask_select` tool that presents the user with a choice.
pub fn create_ask_select_tool() -> AgentTool {
    simple_tool(
        Tool {
            name: "ask_select".into(),
            description: "Ask the user to pick one option from a list. \
                          Use this for yes/no questions, choosing between alternatives, \
                          or any multiple-choice decision."
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
                        "description": "List of choices to present to the user"
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
        |_, args| {
            Box::pin(async move {
                let question = args
                    .get("question")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Choose an option:");
                let options: Vec<String> = args
                    .get("options")
                    .and_then(|v| v.as_array())
                    .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                    .unwrap_or_default();
                let default = args.get("default").and_then(|v| v.as_u64()).map(|i| i as usize);

                if options.is_empty() {
                    return Ok(AgentToolResult::error("No options provided for ask_select"));
                }

                let result = tokio::task::spawn_blocking({
                    let question = question.to_string();
                    let opt_labels: Vec<String> = options
                        .iter()
                        .map(|s| {
                            // Strip leading label notation if any (e.g., "yes" → "yes")
                            s.to_string()
                        })
                        .collect();
                    move || {
                        use dialoguer::Select;
                        Select::new()
                            .with_prompt(&question)
                            .items(&opt_labels)
                            .default(default.unwrap_or(0))
                            .interact()
                    }
                })
                .await
                .map_err(|e| anyhow::anyhow!("ask_select interrupted: {e}"))?
                .map_err(|e| anyhow::anyhow!("ask_select failed: {e}"))?;

                let selected = options.get(result).cloned().unwrap_or_default();
                Ok(AgentToolResult::text(selected))
            })
        },
    )
}

/// Create the `ask_confirm` tool for simple yes/no questions.
pub fn create_ask_confirm_tool() -> AgentTool {
    simple_tool(
        Tool {
            name: "ask_confirm".into(),
            description: "Ask the user a yes/no confirmation question. \
                          Returns 'yes' or 'no'."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "question": {
                        "type": "string",
                        "description": "The yes/no question to ask the user"
                    },
                    "default": {
                        "type": "boolean",
                        "description": "Optional default (true=yes, false=no)"
                    }
                },
                "required": ["question"]
            }),
        },
        "Ask confirmation",
        |_, args| {
            Box::pin(async move {
                let question = args.get("question").and_then(|v| v.as_str()).unwrap_or("Confirm?");

                let result = tokio::task::spawn_blocking({
                    let question = question.to_string();
                    move || {
                        use dialoguer::Confirm;
                        Confirm::new().with_prompt(&question).interact()
                    }
                })
                .await
                .map_err(|e| anyhow::anyhow!("ask_confirm interrupted: {e}"))?
                .map_err(|e| anyhow::anyhow!("ask_confirm failed: {e}"))?;

                Ok(AgentToolResult::text(if result { "yes" } else { "no" }))
            })
        },
    )
}
