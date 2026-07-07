use elph_ai::api::mistral_conversations::{MistralOptions, build_mistral_conversations_payload};
use elph_ai::get_builtin_model;
use elph_ai::types::{Context, Message, Tool, UserContent};
use serde_json::json;

#[test]
fn serializes_nested_tool_parameters_as_plain_json() {
    let model = get_builtin_model("mistral", "devstral-medium-latest").expect("model");
    let context = Context {
        system_prompt: None,
        messages: vec![Message::User {
            content: UserContent::Text("Hi".to_string()),
            timestamp: 0,
        }],
        tools: Some(vec![Tool {
            name: "inspect_schema".to_string(),
            description: "Inspect the schema".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "nested": {
                        "type": "object",
                        "properties": {
                            "value": { "type": "string" }
                        }
                    }
                }
            }),
        }]),
    };
    let payload = build_mistral_conversations_payload(&model, &context, &context.messages, &MistralOptions::default())
        .expect("payload");
    let parameters = &payload["tools"][0]["function"]["parameters"];
    assert_eq!(parameters["type"], "object");
    assert_eq!(parameters["properties"]["nested"]["type"], "object");
    assert_eq!(
        parameters["properties"]["nested"]["properties"]["value"]["type"],
        "string"
    );
    assert!(parameters.as_object().unwrap().keys().all(|k| !k.starts_with('$')));
}
