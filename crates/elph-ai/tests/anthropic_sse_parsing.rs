mod common;

use common::anthropic_model;
use elph_ai::api::anthropic_messages::process_anthropic_sse_buffer;
use elph_ai::types::{AssistantContentBlock, AssistantMessage, StopReason, Usage};
use elph_ai::utils::event_stream::AssistantMessageEventStream;
use serde_json::json;

fn create_sse_buffer(events: &[(&str, &str)]) -> String {
    let mut buffer = events
        .iter()
        .map(|(event, data)| format!("event: {event}\ndata: {data}"))
        .collect::<Vec<_>>()
        .join("\n\n");
    buffer.push_str("\n\n");
    buffer
}

fn empty_output(model: &elph_ai::types::Model) -> AssistantMessage {
    AssistantMessage {
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
    }
}

#[tokio::test]
async fn repairs_malformed_sse_json_and_malformed_streamed_tool_json() {
    let model = anthropic_model("https://api.anthropic.com", None);
    let mut output = empty_output(&model);
    let stream = AssistantMessageEventStream::new();

    let malformed_tool_json_delta = r#"{"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"{\"path\":\"A\H\",\"text\":\"col1\tcol2\"}"}}"#;

    let buffer = create_sse_buffer(&[
        (
            "message_start",
            &json!({
                "type": "message_start",
                "message": {
                    "id": "msg_test",
                    "usage": {
                        "input_tokens": 12,
                        "output_tokens": 0,
                        "cache_read_input_tokens": 0,
                        "cache_creation_input_tokens": 0
                    }
                }
            })
            .to_string(),
        ),
        (
            "content_block_start",
            &json!({
                "type": "content_block_start",
                "index": 0,
                "content_block": {
                    "type": "tool_use",
                    "id": "toolu_test",
                    "name": "edit",
                    "input": {}
                }
            })
            .to_string(),
        ),
        ("content_block_delta", malformed_tool_json_delta),
        (
            "content_block_stop",
            &json!({ "type": "content_block_stop", "index": 0 }).to_string(),
        ),
        (
            "message_delta",
            &json!({
                "type": "message_delta",
                "delta": { "stop_reason": "tool_use" },
                "usage": {
                    "input_tokens": 12,
                    "output_tokens": 5,
                    "cache_read_input_tokens": 0,
                    "cache_creation_input_tokens": 0
                }
            })
            .to_string(),
        ),
        ("message_stop", &json!({ "type": "message_stop" }).to_string()),
    ]);

    process_anthropic_sse_buffer(&buffer, &mut output, &stream, &model)
        .await
        .expect("stream");

    assert_eq!(output.stop_reason, StopReason::ToolUse);
    assert!(output.error_message.is_none());

    let tool_call = match &output.content[0] {
        AssistantContentBlock::ToolCall(tc) => tc,
        other => panic!("expected tool call, got {other:?}"),
    };
    assert_eq!(tool_call.arguments["path"], "A\\H");
    assert_eq!(tool_call.arguments["text"], "col1\tcol2");
}

#[tokio::test]
async fn preserves_refusal_stop_details_from_message_delta() {
    let model = anthropic_model("https://api.anthropic.com", None);
    let mut output = empty_output(&model);
    let stream = AssistantMessageEventStream::new();
    let explanation = "This request triggered restrictions on violative cyber content and was blocked under Anthropic's Usage Policy. To learn more, provide feedback, or request an exemption based on how you use Claude, visit our help center: https://support.claude.com/en/articles/14604842-real-time-cyber-safeguards-on-claude.";

    let buffer = create_sse_buffer(&[
        (
            "message_start",
            &json!({
                "type": "message_start",
                "message": {
                    "id": "msg_01XFUDYJgAACzvnptvVoYEL",
                    "usage": {
                        "input_tokens": 412,
                        "output_tokens": 0,
                        "cache_read_input_tokens": 0,
                        "cache_creation_input_tokens": 0
                    }
                }
            })
            .to_string(),
        ),
        (
            "message_delta",
            &json!({
                "type": "message_delta",
                "delta": {
                    "stop_reason": "refusal",
                    "stop_details": {
                        "type": "refusal",
                        "category": "cyber",
                        "explanation": explanation
                    }
                },
                "usage": {
                    "input_tokens": 412,
                    "output_tokens": 0,
                    "cache_read_input_tokens": 0,
                    "cache_creation_input_tokens": 0
                }
            })
            .to_string(),
        ),
        ("message_stop", &json!({ "type": "message_stop" }).to_string()),
    ]);

    process_anthropic_sse_buffer(&buffer, &mut output, &stream, &model)
        .await
        .expect("stream");

    assert_eq!(output.stop_reason, StopReason::Error);
    assert_eq!(output.error_message.as_deref(), Some(explanation));
}

#[tokio::test]
async fn ignores_unknown_sse_events_after_message_stop() {
    let model = anthropic_model("https://api.anthropic.com", None);
    let mut output = empty_output(&model);
    let stream = AssistantMessageEventStream::new();

    let buffer = create_sse_buffer(&[
        (
            "message_start",
            &json!({
                "type": "message_start",
                "message": {
                    "id": "msg_test",
                    "usage": {
                        "input_tokens": 12,
                        "output_tokens": 0,
                        "cache_read_input_tokens": 0,
                        "cache_creation_input_tokens": 0
                    }
                }
            })
            .to_string(),
        ),
        (
            "content_block_start",
            &json!({
                "type": "content_block_start",
                "index": 0,
                "content_block": { "type": "text", "text": "" }
            })
            .to_string(),
        ),
        (
            "content_block_delta",
            &json!({
                "type": "content_block_delta",
                "index": 0,
                "delta": { "type": "text_delta", "text": "Hello" }
            })
            .to_string(),
        ),
        (
            "content_block_stop",
            &json!({ "type": "content_block_stop", "index": 0 }).to_string(),
        ),
        (
            "message_delta",
            &json!({
                "type": "message_delta",
                "delta": { "stop_reason": "end_turn" },
                "usage": {
                    "input_tokens": 12,
                    "output_tokens": 5,
                    "cache_read_input_tokens": 0,
                    "cache_creation_input_tokens": 0
                }
            })
            .to_string(),
        ),
        ("message_stop", &json!({ "type": "message_stop" }).to_string()),
        ("done", "[DONE]"),
        ("proxy.stats", "not json"),
    ]);

    process_anthropic_sse_buffer(&buffer, &mut output, &stream, &model)
        .await
        .expect("stream");

    assert_eq!(output.stop_reason, StopReason::Stop);
    assert!(output.error_message.is_none());
    assert_eq!(output.content.len(), 1);
    let text = match &output.content[0] {
        AssistantContentBlock::Text(t) => &t.text,
        other => panic!("expected text, got {other:?}"),
    };
    assert_eq!(text, "Hello");
}
