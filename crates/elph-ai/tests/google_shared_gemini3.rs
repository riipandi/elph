use elph_ai::api::google_shared::convert_messages;
use elph_ai::types::{
    AssistantContentBlock, AssistantMessage, Context, Message, Model, ModelCost, StopReason, ToolCall, Usage,
    UserContent,
};
use serde_json::json;

fn gemini_model(api: &str, provider: &str, id: &str) -> Model {
    Model {
        id: id.to_string(),
        name: "Gemini".to_string(),
        api: api.to_string(),
        provider: provider.to_string(),
        base_url: "https://example.com".to_string(),
        reasoning: true,
        thinking_level_map: None,
        input: vec!["text".to_string()],
        cost: ModelCost {
            input: 0.0,
            output: 0.0,
            cache_read: 0.0,
            cache_write: 0.0,

            tiers: None,
        },
        context_window: 128_000,
        max_tokens: 8192,
        headers: None,
        openai_completions_compat: None,
        openai_responses_compat: None,
        anthropic_compat: None,
    }
}

fn context_with_tool_calls(model: &Model, thought_signature: Option<&str>) -> Context {
    let mut first = ToolCall::new("call_1", "bash", json!({ "command": "echo hi" }));
    if let Some(sig) = thought_signature {
        first.thought_signature = Some(sig.to_string());
    }
    Context {
        system_prompt: None,
        messages: vec![
            Message::User {
                content: UserContent::Text("Hi".to_string()),
                timestamp: 0,
            },
            Message::Assistant(AssistantMessage {
                role: "assistant".to_string(),
                content: vec![
                    AssistantContentBlock::ToolCall(first),
                    AssistantContentBlock::ToolCall(ToolCall::new("call_2", "bash", json!({ "command": "ls -la" }))),
                ],
                api: model.api.clone(),
                provider: model.provider.clone(),
                model: model.id.clone(),
                diagnostics: None,
                usage: Usage::default(),
                stop_reason: StopReason::ToolUse,
                timestamp: 1,
                response_id: None,
                response_model: None,
                error_message: None,
            }),
        ],
        tools: None,
    }
}

#[test]
fn does_not_add_skip_validator_for_unsigned_google_gen_ai_tool_calls() {
    let model = gemini_model("google-generative-ai", "google", "gemini-3-pro-preview");
    let other_model = gemini_model("google-generative-ai", "google", "other-model");
    let contents = convert_messages(&model, &context_with_tool_calls(&other_model, None));
    let model_turn = contents.iter().find(|c| c["role"] == "model").expect("model turn");
    let function_calls: Vec<_> = model_turn["parts"]
        .as_array()
        .expect("parts")
        .iter()
        .filter(|p| p.get("functionCall").is_some())
        .collect();
    assert_eq!(function_calls.len(), 2);
    assert!(function_calls[0].get("thoughtSignature").is_none());
    assert!(function_calls[1].get("thoughtSignature").is_none());
    assert!(!model_turn.to_string().contains("skip_thought_signature_validator"));
}

#[test]
fn does_not_add_skip_validator_for_unsigned_vertex_tool_calls() {
    let model = gemini_model("google-vertex", "google-vertex", "gemini-3-pro-preview");
    let contents = convert_messages(&model, &context_with_tool_calls(&model, None));
    let model_turn = contents.iter().find(|c| c["role"] == "model").expect("model turn");
    let function_calls: Vec<_> = model_turn["parts"]
        .as_array()
        .expect("parts")
        .iter()
        .filter(|p| p.get("functionCall").is_some())
        .collect();
    assert_eq!(function_calls.len(), 2);
    assert!(function_calls[0].get("thoughtSignature").is_none());
    assert!(function_calls[1].get("thoughtSignature").is_none());
}

#[test]
fn preserves_valid_thought_signature_for_same_provider_and_model() {
    let model = gemini_model("google-generative-ai", "google", "gemini-3-pro-preview");
    let valid_sig = "AAAAAAAAAAAAAAAAAAAAAA==";
    let contents = convert_messages(&model, &context_with_tool_calls(&model, Some(valid_sig)));
    let model_turn = contents.iter().find(|c| c["role"] == "model").expect("model turn");
    let function_calls: Vec<_> = model_turn["parts"]
        .as_array()
        .expect("parts")
        .iter()
        .filter(|p| p.get("functionCall").is_some())
        .collect();
    assert_eq!(function_calls[0]["thoughtSignature"], valid_sig);
    assert!(function_calls[1].get("thoughtSignature").is_none());
}

#[test]
fn does_not_add_thought_signature_for_non_gemini3_models() {
    let model = gemini_model("google-generative-ai", "google", "gemini-2.5-flash");
    let other_model = gemini_model("google-generative-ai", "google", "other-model");
    let contents = convert_messages(&model, &context_with_tool_calls(&other_model, None));
    let model_turn = contents.iter().find(|c| c["role"] == "model").expect("model turn");
    let function_call = model_turn["parts"]
        .as_array()
        .expect("parts")
        .iter()
        .find(|p| p.get("functionCall").is_some())
        .expect("function call");
    assert!(function_call.get("thoughtSignature").is_none());
}
