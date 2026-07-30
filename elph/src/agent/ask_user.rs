//! Ask-user tool — elph coding agent specific.
//!
//! Supports a legacy single `question` payload or a multi-step `questions` array.
//! Each step can be text, confirm, single select, multi select, or select with an
//! inline custom input (`allow_custom`).

use elph_agent::AgentTool;
use elph_ai::Tool;
use serde_json::Value;
use serde_json::json;
use tokio::sync::{mpsc, oneshot};

use super::events::{AgentUiEvent, UserQuestionOption, UserQuestionRequest, UserQuestionStep};

/// Create the `ask_user_question` tool.
///
/// `ui_tx` is the channel used to present the question to the TUI and await a response.
pub fn create_ask_user_tool(ui_tx: mpsc::UnboundedSender<AgentUiEvent>) -> AgentTool {
    let tx = ui_tx;
    elph_agent::simple_tool(
        Tool {
            name: "ask_user_question".into(),
                constrained_sampling: None,
            description: "Ask the user one or more questions to gather structured input. Use `questions` for multi-step flows. Each step may offer numbered choices, multi-select (`allow_multiple`), and/or an inline custom text field (`allow_custom`). Returns a plain string for a single simple answer, or JSON for multi-step / multi-select results.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "question": {
                        "type": "string",
                        "description": "Single-step question text (legacy; use `questions` for multi-step)"
                    },
                    "questions": {
                        "type": "array",
                        "description": "Ordered list of question steps shown one at a time",
                        "items": {
                            "type": "object",
                            "properties": {
                                "id": {
                                    "type": "string",
                                    "description": "Stable key used in the JSON response object"
                                },
                                "question": {
                                    "type": "string",
                                    "description": "Prompt text for this step"
                                },
                                "options": {
                                    "type": "array",
                                    "items": {
                                        "type": "object",
                                        "properties": {
                                            "value": { "type": "string" },
                                            "label": { "type": "string" },
                                            "hint": {
                                                "type": "string",
                                                "description": "Optional dimmed detail below the label"
                                            }
                                        },
                                        "required": ["value", "label"]
                                    },
                                    "description": "Numbered choices for select / multi-select steps"
                                },
                                "allow_multiple": {
                                    "type": "boolean",
                                    "description": "When true, the user may pick more than one option (Space toggles, Enter submits)"
                                },
                                "allow_custom": {
                                    "type": "boolean",
                                    "description": "Show an inline text field below choices for a custom answer"
                                },
                                "custom_label": {
                                    "type": "string",
                                    "description": "Placeholder for the inline custom input (default: Other…)"
                                },
                                "default": {
                                    "description": "Optional default (boolean for confirm-without-options, string, or JSON array for multi-select)"
                                },
                                "required": {
                                    "type": "boolean",
                                    "description": "When false, Esc skips this step with an empty answer (default: true)"
                                },
                                "min_length": {
                                    "type": "integer",
                                    "description": "Minimum character count for free-text answers"
                                },
                                "pattern": {
                                    "type": "string",
                                    "description": "Regex pattern free-text answers must match"
                                },
                                "tab_label": {
                                    "type": "string",
                                    "description": "Short label for the header tab in multi-step flows"
                                }
                            },
                            "required": ["question"]
                        }
                    },
                    "options": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "value": { "type": "string" },
                                "label": { "type": "string" },
                                "hint": {
                                    "type": "string",
                                    "description": "Optional dimmed detail below the label"
                                }
                            },
                            "required": ["value", "label"]
                        },
                        "description": "Choices for legacy single-step select mode"
                    },
                    "allow_multiple": {
                        "type": "boolean",
                        "description": "Legacy single-step multi-select flag"
                    },
                    "allow_custom": {
                        "type": "boolean",
                        "description": "Legacy single-step inline custom input flag"
                    },
                    "custom_label": {
                        "type": "string",
                        "description": "Legacy single-step custom input placeholder"
                    },
                    "default": {
                        "description": "Optional default value (boolean for confirm, string for text/select)"
                    },
                    "required": {
                        "type": "boolean",
                        "description": "Legacy single-step required flag (default: true)"
                    },
                    "min_length": {
                        "type": "integer",
                        "description": "Legacy single-step minimum text length"
                    },
                    "pattern": {
                        "type": "string",
                        "description": "Legacy single-step regex pattern"
                    },
                    "tab_label": {
                        "type": "string",
                        "description": "Legacy single-step header tab label"
                    }
                },
                "anyOf": [
                    { "required": ["question"] },
                    { "required": ["questions"] }
                ]
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
    let steps = parse_question_steps(&args)?;

    let (response_tx, response_rx) = oneshot::channel();

    let request = UserQuestionRequest { steps, response_tx };

    ui_tx
        .send(AgentUiEvent::UserQuestionRequired(request))
        .map_err(|_| anyhow::anyhow!("UI channel closed"))?;

    let answer = response_rx
        .await
        .map_err(|_| anyhow::anyhow!("User question response channel closed"))?;

    Ok(elph_agent::AgentToolResult::text(answer))
}

fn parse_question_steps(args: &Value) -> anyhow::Result<Vec<UserQuestionStep>> {
    for key in ["questions", "steps", "items"] {
        if let Some(items) = args.get(key).and_then(Value::as_array)
            && items.is_empty()
        {
            anyhow::bail!("`{key}` must contain at least one item");
        }
    }

    if let Some(items) = questions_array_from_args(args) {
        return items
            .iter()
            .enumerate()
            .map(|(index, item)| parse_question_step(item, index))
            .collect();
    }

    let question = single_question_text_from_args(args).ok_or_else(|| missing_question_error(args))?;

    Ok(vec![legacy_single_step(args, question)])
}

fn questions_array_from_args(args: &Value) -> Option<Vec<Value>> {
    let raw = args
        .get("questions")
        .or_else(|| args.get("steps"))
        .or_else(|| args.get("items"))?;
    questions_array_from_value(raw)
}

fn questions_array_from_value(raw: &Value) -> Option<Vec<Value>> {
    if let Some(items) = raw.as_array() {
        return (!items.is_empty()).then(|| items.clone());
    }
    if raw.is_object() {
        return Some(vec![raw.clone()]);
    }
    if let Some(text) = raw.as_str() {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return None;
        }
        if let Ok(parsed) = serde_json::from_str::<Value>(trimmed) {
            return questions_array_from_value(&parsed);
        }
    }
    None
}

fn single_question_text_from_args(args: &Value) -> Option<String> {
    for key in ["question", "prompt", "message", "text"] {
        if let Some(text) = args.get(key).and_then(Value::as_str) {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

fn missing_question_error(args: &Value) -> anyhow::Error {
    let keys = args
        .as_object()
        .map(|fields| fields.keys().cloned().collect::<Vec<_>>().join(", "))
        .unwrap_or_default();
    let detail = if keys.is_empty() {
        "no arguments were provided".to_string()
    } else {
        format!("received keys: {keys}")
    };
    anyhow::anyhow!("Missing required argument: `question` (string) or `questions` (array). {detail}")
}

fn legacy_single_step(args: &Value, question: String) -> UserQuestionStep {
    UserQuestionStep {
        id: "answer".into(),
        question,
        options: parse_options(args.get("options")),
        allow_multiple: args.get("allow_multiple").and_then(Value::as_bool).unwrap_or(false),
        allow_custom: args.get("allow_custom").and_then(Value::as_bool).unwrap_or(false),
        custom_label: args
            .get("custom_label")
            .and_then(Value::as_str)
            .unwrap_or("Other…")
            .to_string(),
        default: parse_default(args.get("default")),
        required: args.get("required").and_then(Value::as_bool).unwrap_or(true),
        min_length: args.get("min_length").and_then(Value::as_u64).map(|n| n as usize),
        pattern: args.get("pattern").and_then(Value::as_str).map(str::to_string),
        tab_label: args.get("tab_label").and_then(Value::as_str).map(str::to_string),
    }
}

fn parse_question_step(value: &Value, index: usize) -> anyhow::Result<UserQuestionStep> {
    let question = value
        .get("question")
        .or_else(|| value.get("prompt"))
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("Question step {} is missing `question`", index + 1))?
        .to_string();

    let id = value
        .get("id")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| format!("q{}", index + 1));

    Ok(UserQuestionStep {
        id,
        question,
        options: parse_options(value.get("options")),
        allow_multiple: value.get("allow_multiple").and_then(Value::as_bool).unwrap_or(false),
        allow_custom: value.get("allow_custom").and_then(Value::as_bool).unwrap_or(false),
        custom_label: value
            .get("custom_label")
            .and_then(Value::as_str)
            .unwrap_or("Other…")
            .to_string(),
        default: parse_default(value.get("default")),
        required: value.get("required").and_then(Value::as_bool).unwrap_or(true),
        min_length: value.get("min_length").and_then(Value::as_u64).map(|n| n as usize),
        pattern: value.get("pattern").and_then(Value::as_str).map(str::to_string),
        tab_label: value.get("tab_label").and_then(Value::as_str).map(str::to_string),
    })
}

fn parse_options(value: Option<&Value>) -> Option<Vec<UserQuestionOption>> {
    let arr = value?.as_array()?;
    let options = arr
        .iter()
        .filter_map(|item| {
            let val = item.get("value")?.as_str()?.to_string();
            let label = item.get("label").and_then(Value::as_str).unwrap_or(&val).to_string();
            let hint = item
                .get("hint")
                .or_else(|| item.get("description"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|text| !text.is_empty())
                .map(str::to_string);
            Some(UserQuestionOption {
                value: val,
                label,
                hint,
            })
        })
        .collect::<Vec<_>>();
    (!options.is_empty()).then_some(options)
}

fn parse_default(value: Option<&Value>) -> Option<String> {
    let value = value?;
    if let Some(flag) = value.as_bool() {
        return Some(flag.to_string());
    }
    if value.is_array() || value.is_object() {
        return serde_json::to_string(value).ok();
    }
    Some(value.to_string().trim_matches('"').to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_single_question_parses() {
        let args = json!({
            "question": "Pick one",
            "options": [{ "value": "a", "label": "Alpha" }],
            "allow_custom": true
        });
        let steps = parse_question_steps(&args).unwrap();
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0].id, "answer");
        assert!(steps[0].allow_custom);
        assert_eq!(steps[0].options.as_ref().unwrap().len(), 1);
    }

    #[test]
    fn questions_array_parses_multi_step() {
        let args = json!({
            "questions": [
                { "id": "color", "question": "Color?", "options": [{ "value": "red", "label": "Red" }] },
                { "id": "note", "question": "Any notes?" }
            ]
        });
        let steps = parse_question_steps(&args).unwrap();
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0].id, "color");
        assert!(steps[1].options.is_none());
    }

    #[test]
    fn step_allow_multiple_flag_parses() {
        let args = json!({
            "questions": [{
                "question": "Tags?",
                "options": [{ "value": "a", "label": "A" }, { "value": "b", "label": "B" }],
                "allow_multiple": true
            }]
        });
        let steps = parse_question_steps(&args).unwrap();
        assert!(steps[0].allow_multiple);
    }

    #[test]
    fn prompt_alias_parses_as_single_question() {
        let args = json!({ "prompt": "Pick a color" });
        let steps = parse_question_steps(&args).unwrap();
        assert_eq!(steps[0].question, "Pick a color");
    }

    #[test]
    fn steps_alias_parses_multi_step() {
        let args = json!({
            "steps": [
                { "question": "First?" },
                { "question": "Second?" }
            ]
        });
        let steps = parse_question_steps(&args).unwrap();
        assert_eq!(steps.len(), 2);
    }

    #[test]
    fn single_object_questions_array_parses() {
        let args = json!({
            "questions": { "question": "Only one?" }
        });
        let steps = parse_question_steps(&args).unwrap();
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0].question, "Only one?");
    }

    #[test]
    fn missing_question_error_lists_received_keys() {
        let err = parse_question_steps(&json!({ "options": [] })).unwrap_err();
        assert!(err.to_string().contains("question"));
        assert!(err.to_string().contains("options"));
    }
}
