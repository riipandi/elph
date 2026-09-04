mod common;

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;

use axum::Router;
use axum::body::Body;
use axum::http::HeaderMap;
use axum::http::header;
use axum::http::{HeaderValue, StatusCode};
use axum::response::Response;
use axum::routing::post;
use common::{anthropic_model, sample_user_context};
use elph_ai::api::anthropic_messages::AnthropicMessagesApi;
use elph_ai::api::anthropic_messages::AnthropicOptions;
use elph_ai::types::StreamOptions;
use futures::stream;
use tokio::net::TcpListener;

type Captured = Arc<Mutex<Vec<String>>>;

const SSE_EVENTS: &[&str] = &[
    r#"event: message_start
data: {"type":"message_start","message":{"id":"msg_1","usage":{"input_tokens":1,"output_tokens":0}}}"#,
    r#"event: content_block_start
data: {"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}"#,
    r#"event: content_block_delta
data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"hi"}}"#,
    r#"event: content_block_stop
data: {"type":"content_block_stop","index":0}"#,
    r#"event: message_delta
data: {"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":1}}"#,
    r#"event: message_stop
data: {"type":"message_stop"}"#,
];

async fn sse_response() -> Response {
    let body = Body::from_stream(stream::iter(
        SSE_EVENTS
            .iter()
            .map(|event| Ok::<_, std::convert::Infallible>(format!("{event}\n\n"))),
    ));
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, HeaderValue::from_static("text/event-stream"))
        .body(body)
        .expect("response")
}

async fn start_server(captured: Captured) -> String {
    let app = Router::new().route(
        "/v1/messages",
        post(move |headers: HeaderMap| {
            let captured = captured.clone();
            async move {
                if let Some(version) = headers.get("anthropic-version").and_then(|v| v.to_str().ok()) {
                    captured.lock().expect("captured").push(version.to_string());
                }
                sse_response().await
            }
        }),
    );
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");
    tokio::spawn(async move {
        axum::serve(listener, app).await.expect("serve");
    });
    format!("http://{addr}")
}

/// Streams against a local mock and returns the `anthropic-version` values seen
/// by the server (one entry per request).
async fn run_stream(model_headers: HashMap<String, String>, options: AnthropicOptions) -> Vec<String> {
    let captured: Captured = Arc::new(Mutex::new(Vec::new()));
    let url = start_server(captured.clone()).await;

    let mut model = anthropic_model(&url, None);
    model.headers = Some(model_headers);

    let stream = AnthropicMessagesApi.stream_with_options(&model, &sample_user_context(Some("system")), options);
    let result = stream.result().await;
    assert!(result.error_message.is_none(), "stream failed: {:?}", result.error_message);

    let versions = captured.lock().expect("captured").clone();
    assert_eq!(versions.len(), 1, "expected exactly one request");
    versions
}

#[tokio::test]
async fn sends_anthropic_version_when_auth_comes_from_headers() {
    let mut model_headers = HashMap::new();
    model_headers.insert("authorization".to_string(), "Bearer gateway-key".to_string());

    let captured = run_stream(model_headers, AnthropicOptions::default()).await;
    assert_eq!(captured, vec!["2023-06-01".to_string()]);
}

#[tokio::test]
async fn sends_anthropic_version_for_oauth_bearer_keys() {
    let options = AnthropicOptions {
        base: StreamOptions {
            api_key: Some("sk-ant-oat01-test".to_string()),
            ..Default::default()
        },
        ..Default::default()
    };
    let captured = run_stream(HashMap::new(), options).await;
    assert_eq!(captured, vec!["2023-06-01".to_string()]);
}

#[tokio::test]
async fn respects_caller_provided_anthropic_version() {
    let mut headers: HashMap<String, Option<String>> = HashMap::new();
    headers.insert("anthropic-version".to_string(), Some("2099-01-01".to_string()));
    let options = AnthropicOptions {
        base: StreamOptions {
            api_key: Some("key".to_string()),
            headers: Some(headers),
            ..Default::default()
        },
        ..Default::default()
    };
    let captured = run_stream(HashMap::new(), options).await;
    assert_eq!(captured, vec!["2099-01-01".to_string()]);
}
