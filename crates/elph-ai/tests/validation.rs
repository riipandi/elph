use elph_ai::types::{Tool, ToolCall};
use elph_ai::utils::validation::validate_tool_call;
use serde_json::json;

fn tool_with_schema(schema: serde_json::Value) -> Tool {
    Tool {
        name: "echo".to_string(),
        description: "Echo tool".to_string(),
        parameters: schema,
    }
}

#[test]
fn validates_required_number_field() {
    let tool = tool_with_schema(json!({
        "type": "object",
        "properties": { "count": { "type": "number" } },
        "required": ["count"]
    }));
    let call = ToolCall::new("tool-1", "echo", json!({ "count": 42 }));
    assert!(validate_tool_call(&tool, &call).is_ok());
}

#[test]
fn rejects_wrong_type_for_number_field() {
    let tool = tool_with_schema(json!({
        "type": "object",
        "properties": { "count": { "type": "number" } },
        "required": ["count"]
    }));
    let call = ToolCall::new("tool-1", "echo", json!({ "count": "42" }));
    assert!(validate_tool_call(&tool, &call).is_err());
}
