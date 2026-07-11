//! Argument parsing and validation for ask_* tools.

use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AskTextArgs {
    pub question: String,
    pub default: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AskSelectArgs {
    pub question: String,
    pub options: Vec<String>,
    pub default_index: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AskConfirmArgs {
    pub question: String,
    pub default: bool,
}

pub fn parse_ask_text_args(args: &Value) -> Result<AskTextArgs, String> {
    let question = required_string(args, "question")?;
    if question.trim().is_empty() {
        return Err("question must not be empty".to_string());
    }
    let default = optional_string(args, "default");
    Ok(AskTextArgs { question, default })
}

pub fn parse_ask_select_args(args: &Value) -> Result<AskSelectArgs, String> {
    let question = required_string(args, "question")?;
    if question.trim().is_empty() {
        return Err("question must not be empty".to_string());
    }
    let options = args
        .get("options")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(str::trim).filter(|s| !s.is_empty()).map(str::to_string))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if options.is_empty() {
        return Err("options must contain at least one non-empty string".to_string());
    }
    let default_index = args
        .get("default")
        .and_then(|v| v.as_u64())
        .map(|i| i as usize)
        .unwrap_or(0);
    if default_index >= options.len() {
        return Err(format!(
            "default index {default_index} is out of range for {} option(s)",
            options.len()
        ));
    }
    Ok(AskSelectArgs {
        question,
        options,
        default_index,
    })
}

pub fn parse_ask_confirm_args(args: &Value) -> Result<AskConfirmArgs, String> {
    let question = required_string(args, "question")?;
    if question.trim().is_empty() {
        return Err("question must not be empty".to_string());
    }
    let default = args.get("default").and_then(|v| v.as_bool()).unwrap_or(false);
    Ok(AskConfirmArgs { question, default })
}

/// Human-readable args summary for transcript / checkpoint writes.
pub fn format_args_summary(tool_name: &str, args: &Value) -> String {
    match tool_name {
        "ask_text" => parse_ask_text_args(args)
            .map(|a| format_question_summary(&a.question, a.default.as_deref()))
            .unwrap_or_else(|e| format!("invalid args: {e}")),
        "ask_select" => parse_ask_select_args(args)
            .map(|a| {
                let opts = a.options.join(", ");
                format!("{} [options: {opts}]", a.question)
            })
            .unwrap_or_else(|e| format!("invalid args: {e}")),
        "ask_confirm" => parse_ask_confirm_args(args)
            .map(|a| a.question)
            .unwrap_or_else(|e| format!("invalid args: {e}")),
        _ => args.to_string(),
    }
}

fn format_question_summary(question: &str, default: Option<&str>) -> String {
    match default {
        Some(d) if !d.is_empty() => format!("{question} (default: {d})"),
        _ => question.to_string(),
    }
}

fn required_string(args: &Value, field: &str) -> Result<String, String> {
    args.get(field)
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .ok_or_else(|| format!("{field} is required"))
}

fn optional_string(args: &Value, field: &str) -> Option<String> {
    args.get(field)
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_ask_text_requires_question() {
        assert!(parse_ask_text_args(&json!({})).is_err());
        assert!(parse_ask_text_args(&json!({ "question": "  " })).is_err());
    }

    #[test]
    fn parse_ask_text_accepts_default() {
        let args = parse_ask_text_args(&json!({ "question": "Name?", "default": "anon" })).expect("parsed");
        assert_eq!(args.question, "Name?");
        assert_eq!(args.default.as_deref(), Some("anon"));
    }

    #[test]
    fn parse_ask_select_rejects_empty_options() {
        assert!(parse_ask_select_args(&json!({ "question": "Pick", "options": [] })).is_err());
    }

    #[test]
    fn parse_ask_select_rejects_out_of_range_default() {
        assert!(
            parse_ask_select_args(&json!({
                "question": "Pick",
                "options": ["a"],
                "default": 3
            }))
            .is_err()
        );
    }

    #[test]
    fn parse_ask_select_filters_blank_options() {
        let args = parse_ask_select_args(&json!({
            "question": "Pick",
            "options": ["yes", " ", "no"]
        }))
        .expect("parsed");
        assert_eq!(args.options, vec!["yes".to_string(), "no".to_string()]);
    }

    #[test]
    fn parse_ask_confirm_uses_default_false() {
        let args = parse_ask_confirm_args(&json!({ "question": "Proceed?" })).expect("parsed");
        assert!(!args.default);
    }

    #[test]
    fn format_args_summary_for_ask_text() {
        let summary = format_args_summary("ask_text", &json!({ "question": "Continue?", "default": "yes" }));
        assert_eq!(summary, "Continue? (default: yes)");
    }
}
