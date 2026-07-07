use elph_ai::api::google_shared::convert_tools;
use elph_ai::types::Tool;
use serde_json::json;

#[test]
fn strips_json_schema_meta_keys_from_parameters() {
    let tools = vec![Tool {
        name: "search".to_string(),
        description: "Search the web".to_string(),
        parameters: json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "properties": { "query": { "type": "string" } },
            "required": ["query"]
        }),
    }];
    let converted = convert_tools(&tools, true).expect("tools");
    let params = &converted[0]["functionDeclarations"][0]["parameters"];
    assert!(params.get("$schema").is_none());
    assert_eq!(params["type"], "object");
    assert_eq!(params["properties"]["query"]["type"], "string");
}

#[test]
fn returns_none_for_empty_tool_list() {
    assert!(convert_tools(&[], true).is_none());
}
