//! Tool card argument and output formatting.

pub const TOOL_OUTPUT_MAX_LINES: usize = 12;
pub const TOOL_OUTPUT_MAX_CHARS: usize = 1_500;

pub fn format_tool_args_display(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) else {
        return trimmed.to_string();
    };
    format_tool_args_json(&value)
}

fn format_tool_args_json(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Object(map) if map.is_empty() => String::new(),
        serde_json::Value::Object(map) if map.len() == 1 => {
            map.values().next().map(format_json_scalar).unwrap_or_default()
        }
        serde_json::Value::Object(map) => map
            .iter()
            .map(|(key, val)| format!("{key}: {}", format_json_scalar(val)))
            .collect::<Vec<_>>()
            .join(", "),
        other => format_json_scalar(other),
    }
}

fn format_json_scalar(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(text) => text.clone(),
        serde_json::Value::Number(num) => num.to_string(),
        serde_json::Value::Bool(flag) => flag.to_string(),
        serde_json::Value::Null => "null".to_string(),
        serde_json::Value::Array(items) => {
            let parts: Vec<String> = items.iter().map(format_json_scalar).collect();
            parts.join(", ")
        }
        serde_json::Value::Object(_) => serde_json::to_string(value).unwrap_or_default(),
    }
}

pub fn format_tool_output_display(output: &str) -> String {
    let trimmed = output.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if trimmed.chars().count() <= TOOL_OUTPUT_MAX_CHARS {
        let lines: Vec<&str> = trimmed.lines().collect();
        if lines.len() <= TOOL_OUTPUT_MAX_LINES {
            return trimmed.to_string();
        }
        let mut body = lines
            .iter()
            .take(TOOL_OUTPUT_MAX_LINES)
            .copied()
            .collect::<Vec<_>>()
            .join("\n");
        body.push_str(&format!("\n… ({line_count} lines total)", line_count = lines.len()));
        return body;
    }
    let truncated: String = trimmed.chars().take(TOOL_OUTPUT_MAX_CHARS.saturating_sub(1)).collect();
    format!("{truncated}…")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_args_json_single_key_shows_value_only() {
        assert_eq!(format_tool_args_display(r#"{"path":"src/lib.rs"}"#), "src/lib.rs");
    }

    #[test]
    fn tool_output_truncates_long_bodies() {
        let long = "line\n".repeat(20);
        let display = format_tool_output_display(&long);
        assert!(display.contains("lines total"));
    }
}
