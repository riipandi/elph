use elph_ai::api::openai_responses_shared::process_responses_stream;
use elph_ai::types::{AssistantContentBlock, AssistantMessage, Model, ModelCost, StopReason, Usage};
use elph_ai::utils::event_stream::AssistantMessageEventStream;
use serde_json::json;

fn responses_model() -> Model {
    Model {
        id: "gpt-5-mini".to_string(),
        name: "GPT-5 Mini".to_string(),
        api: "openai-responses".to_string(),
        provider: "openai".to_string(),
        base_url: "https://api.openai.com/v1".to_string(),
        reasoning: true,
        thinking_level_map: None,
        input: vec!["text".to_string()],
        cost: ModelCost {
            input: 0.0,
            output: 0.0,
            cache_read: 0.0,
            cache_write: 0.0,
        },
        context_window: 400_000,
        max_tokens: 128_000,
        headers: None,
        openai_completions_compat: None,
        openai_responses_compat: None,
        anthropic_compat: None,
    }
}

fn function_call_events(arguments_json: &str) -> Vec<serde_json::Value> {
    vec![
        json!({
            "type": "response.output_item.added",
            "output_index": 0,
            "item": {
                "type": "function_call",
                "id": "fc_test",
                "call_id": "call_test",
                "name": "edit",
                "arguments": ""
            }
        }),
        json!({
            "type": "response.function_call_arguments.delta",
            "output_index": 0,
            "delta": "{\"path\":\"README.md\""
        }),
        json!({
            "type": "response.function_call_arguments.delta",
            "output_index": 0,
            "delta": ",\"content\":\"updated\"}"
        }),
        json!({
            "type": "response.function_call_arguments.done",
            "output_index": 0,
            "arguments": arguments_json
        }),
        json!({
            "type": "response.output_item.done",
            "output_index": 0,
            "item": {
                "type": "function_call",
                "id": "fc_test",
                "call_id": "call_test",
                "name": "edit",
                "arguments": arguments_json
            }
        }),
        json!({
            "type": "response.completed",
            "response": { "id": "resp_test", "status": "completed" }
        }),
    ]
}

#[tokio::test]
async fn persists_parsed_tool_call_arguments_without_partial_json_scratch() {
    let model = responses_model();
    let mut output = AssistantMessage {
        role: "assistant".to_string(),
        content: vec![],
        api: model.api.clone(),
        provider: model.provider.clone(),
        model: model.id.clone(),
        usage: Usage::default(),
        stop_reason: StopReason::Stop,
        timestamp: 0,
        response_id: None,
        response_model: None,
        error_message: None,
    };
    let stream = AssistantMessageEventStream::new();
    let arguments_json = r#"{"path":"README.md","content":"updated"}"#;

    process_responses_stream(function_call_events(arguments_json), &mut output, &stream, &model, None)
        .await
        .expect("stream");

    assert_eq!(output.content.len(), 1);
    let tool_call = match &output.content[0] {
        AssistantContentBlock::ToolCall(tc) => tc,
        other => panic!("expected tool call, got {other:?}"),
    };
    assert_eq!(tool_call.arguments["path"], "README.md");
    assert_eq!(tool_call.arguments["content"], "updated");
    assert!(tool_call.thought_signature.is_none());
}
