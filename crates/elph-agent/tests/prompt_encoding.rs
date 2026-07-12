use elph_agent::{AgentToolResult, ToolResultContent};
use elph_agent::{
    PromptEncodingConfig, PromptEncodingDelimiter, PromptEncodingMode, apply_to_tool_result, decode_toon_fence,
    encode_value,
};
use elph_ai::TextContent;
use serde_json::json;

fn toon_config() -> PromptEncodingConfig {
    PromptEncodingConfig {
        mode: PromptEncodingMode::Toon,
        min_bytes: 1,
        min_savings_ratio: 1.05,
        ..PromptEncodingConfig::default()
    }
}

#[test]
fn mcp_structured_content_roundtrip() {
    let catalog = json!([
        { "title": "Intro", "url": "https://example.com/intro" },
        { "title": "API", "url": "https://example.com/api" }
    ]);

    let mut result = AgentToolResult {
        content: vec![ToolResultContent::Text(TextContent::new("preview"))],
        details: json!({
            "mcp": true,
            "structured_content": catalog
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
    assert!(text.contains("2-space indent"));

    let decoded = decode_toon_fence(text).expect("decode");
    assert_eq!(decoded, catalog);
}

#[test]
fn tabular_tool_json_encodes_with_tab_delimiter() {
    let value = json!([
        { "sku": "A1", "qty": 2, "price": 9.99 },
        { "sku": "B2", "qty": 1, "price": 14.5 }
    ]);
    let encoded = encode_value(&value, &toon_config()).expect("encoded");
    assert!(encoded.contains("tab-separated"));
}

#[test]
fn env_delimiter_override_uses_pipe() {
    let value = json!([{ "id": 1, "name": "a" }, { "id": 2, "name": "b" }]);
    let config = PromptEncodingConfig {
        mode: PromptEncodingMode::Toon,
        min_bytes: 1,
        min_savings_ratio: 1.05,
        tabular_delimiter: Some(PromptEncodingDelimiter::Pipe),
        ..PromptEncodingConfig::default()
    };
    let encoded = encode_value(&value, &config).expect("encoded");
    let body = elph_agent::parse_toon_fence(&encoded).expect("body");
    assert!(body.contains('|'));
}
