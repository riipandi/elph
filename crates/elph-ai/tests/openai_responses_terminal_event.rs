use elph_ai::api::openai_responses_shared::process_responses_stream;
use elph_ai::types::{AssistantMessage, Model, ModelCost, StopReason, Usage};
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

fn empty_output(model: &Model) -> AssistantMessage {
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
async fn rejects_streams_that_end_before_a_terminal_response_event() {
    let model = responses_model();
    let mut output = empty_output(&model);
    let stream = AssistantMessageEventStream::new();
    let events = vec![
        json!({
            "type": "response.created",
            "response": { "id": "resp_early_eof" }
        }),
        json!({
            "type": "response.output_item.added",
            "output_index": 0,
            "item": { "type": "reasoning", "id": "rs_early_eof", "summary": [] }
        }),
        json!({
            "type": "response.reasoning_text.delta",
            "output_index": 0,
            "delta": "partial reasoning before the stream ends"
        }),
    ];

    let err = process_responses_stream(events, &mut output, &stream, &model, None)
        .await
        .expect_err("expected terminal event error");

    assert_eq!(
        err.to_string(),
        "OpenAI Responses stream ended before a terminal response event"
    );
}

#[tokio::test]
async fn finalizes_completed_terminal_events_as_stop() {
    let model = responses_model();
    let mut output = empty_output(&model);
    let stream = AssistantMessageEventStream::new();
    let events = vec![json!({
        "type": "response.completed",
        "response": {
            "id": "resp_completed",
            "status": "completed",
            "usage": {
                "input_tokens": 20,
                "output_tokens": 7,
                "total_tokens": 27,
                "input_tokens_details": { "cached_tokens": 2 }
            }
        }
    })];

    process_responses_stream(events, &mut output, &stream, &model, None)
        .await
        .expect("stream");

    assert_eq!(output.response_id.as_deref(), Some("resp_completed"));
    assert_eq!(output.stop_reason, StopReason::Stop);
    assert_eq!(output.usage.input, 18);
    assert_eq!(output.usage.output, 7);
    assert_eq!(output.usage.cache_read, 2);
    assert_eq!(output.usage.cache_write, 0);
    assert_eq!(output.usage.total_tokens, 27);
}

#[tokio::test]
async fn finalizes_incomplete_terminal_events_as_length() {
    let model = responses_model();
    let mut output = empty_output(&model);
    let stream = AssistantMessageEventStream::new();
    let events = vec![json!({
        "type": "response.incomplete",
        "response": {
            "id": "resp_incomplete",
            "status": "incomplete",
            "usage": {
                "input_tokens": 30,
                "output_tokens": 12,
                "total_tokens": 42,
                "input_tokens_details": { "cached_tokens": 5 }
            }
        }
    })];

    process_responses_stream(events, &mut output, &stream, &model, None)
        .await
        .expect("stream");

    assert_eq!(output.response_id.as_deref(), Some("resp_incomplete"));
    assert_eq!(output.stop_reason, StopReason::Length);
    assert_eq!(output.usage.input, 25);
    assert_eq!(output.usage.output, 12);
    assert_eq!(output.usage.cache_read, 5);
    assert_eq!(output.usage.cache_write, 0);
    assert_eq!(output.usage.total_tokens, 42);
}

#[tokio::test]
async fn rejects_failed_terminal_events_with_provider_error() {
    let model = responses_model();
    let mut output = empty_output(&model);
    let stream = AssistantMessageEventStream::new();
    let events = vec![json!({
        "type": "response.failed",
        "response": {
            "id": "resp_failed",
            "status": "failed",
            "error": { "code": "server_error", "message": "boom" }
        }
    })];

    let err = process_responses_stream(events, &mut output, &stream, &model, None)
        .await
        .expect_err("expected provider error");

    assert_eq!(err.to_string(), "server_error: boom");
}
