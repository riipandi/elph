//! Apply optional TOON encoding to tool results before they enter LLM context.

use elph_ai::TextContent;
use serde_json::Value;

use crate::types::{AgentToolResult, ToolResultContent};

use super::config::PromptEncodingConfig;
use super::encode::{already_toon_encoded, encode_value};

/// Rewrite eligible tool-result text blocks using TOON (model-visible `content` only).
pub fn apply_to_tool_result(result: &mut AgentToolResult, config: &PromptEncodingConfig) {
    if !config.is_enabled() {
        return;
    }

    if config.targets.structured_details {
        apply_structured_details(result, config);
    }
    if config.targets.tool_result_text {
        apply_tool_result_text(result, config);
    }
}

fn apply_structured_details(result: &mut AgentToolResult, config: &PromptEncodingConfig) {
    let Some(structured) = structured_content_value(&result.details) else {
        return;
    };
    if structured.is_null() {
        return;
    }
    let Some(encoded) = encode_value(structured, config) else {
        return;
    };
    replace_primary_text(result, encoded);
}

fn apply_tool_result_text(result: &mut AgentToolResult, config: &PromptEncodingConfig) {
    for block in &mut result.content {
        let ToolResultContent::Text(text) = block else {
            continue;
        };
        if already_toon_encoded(&text.text) {
            continue;
        }
        let Ok(value) = serde_json::from_str::<Value>(&text.text) else {
            continue;
        };
        let Some(encoded) = encode_value(&value, config) else {
            continue;
        };
        *text = TextContent::new(encoded);
    }
}

fn structured_content_value(details: &Value) -> Option<&Value> {
    details.get("structured_content").filter(|v| !v.is_null())
}

fn replace_primary_text(result: &mut AgentToolResult, encoded: String) {
    if let Some(ToolResultContent::Text(text)) = result.content.first_mut() {
        text.text = encoded;
        return;
    }
    result
        .content
        .insert(0, ToolResultContent::Text(TextContent::new(encoded)));
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn toon_config() -> PromptEncodingConfig {
        PromptEncodingConfig {
            mode: super::super::config::PromptEncodingMode::Toon,
            min_bytes: 1,
            ..PromptEncodingConfig::default()
        }
    }

    #[test]
    fn off_mode_is_noop() {
        let mut result = AgentToolResult::text(r#"[{"id":1},{"id":2}]"#);
        apply_to_tool_result(&mut result, &PromptEncodingConfig::default());
        let text = match &result.content[0] {
            ToolResultContent::Text(t) => t.text.as_str(),
            _ => panic!("expected text"),
        };
        assert!(!text.contains("```toon"));
    }

    #[test]
    fn plain_text_is_unchanged() {
        let mut result = AgentToolResult::text("line one\nline two");
        apply_to_tool_result(&mut result, &toon_config());
        let text = match &result.content[0] {
            ToolResultContent::Text(t) => t.text.as_str(),
            _ => panic!("expected text"),
        };
        assert_eq!(text, "line one\nline two");
    }

    #[test]
    fn encodes_mcp_structured_details() {
        let mut result = AgentToolResult {
            content: vec![ToolResultContent::Text(TextContent::new("preview"))],
            details: json!({
                "mcp": true,
                "structured_content": [
                    { "title": "A", "url": "https://a" },
                    { "title": "B", "url": "https://b" }
                ]
            }),
            added_tool_names: None,
            terminate: None,
        };
        apply_to_tool_result(&mut result, &toon_config());
        let text = match &result.content[0] {
            ToolResultContent::Text(t) => t.text.as_str(),
            _ => panic!("expected text"),
        };
        assert!(text.contains("```toon"));
    }

    #[test]
    fn min_bytes_gate_blocks_small_payload() {
        let mut result = AgentToolResult::text(r#"[{"id":1},{"id":2}]"#);
        let config = PromptEncodingConfig {
            mode: super::super::config::PromptEncodingMode::Toon,
            min_bytes: 10_000,
            ..PromptEncodingConfig::default()
        };
        apply_to_tool_result(&mut result, &config);
        let text = match &result.content[0] {
            ToolResultContent::Text(t) => t.text.as_str(),
            _ => panic!("expected text"),
        };
        assert!(!text.contains("```toon"));
    }
}
