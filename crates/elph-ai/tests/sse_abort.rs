use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use axum::{
    Router,
    body::Body,
    http::{HeaderValue, StatusCode, header},
    response::Response,
    routing::get,
};
use elph_ai::api::common::{REQUEST_ABORTED, build_http_client};
use elph_ai::api::sse::{for_each_anthropic_sse_event, for_each_sse_json_event};
use futures::stream;
use serde_json::json;
use tokio::net::TcpListener;
use tokio::time::{Duration, sleep};
use tokio_util::sync::CancellationToken;

async fn slow_sse_handler() -> Response {
    let body = Body::from_stream(stream::unfold(0u32, |index| async move {
        if index >= 20 {
            return None;
        }
        let payload = json!({ "index": index });
        sleep(Duration::from_millis(50)).await;
        Some((Ok::<_, std::convert::Infallible>(format!("data: {payload}\n\n")), index + 1))
    }));

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, HeaderValue::from_static("text/event-stream"))
        .body(body)
        .expect("response")
}

async fn slow_anthropic_sse_handler() -> Response {
    let body = Body::from_stream(stream::unfold(0u32, |index| async move {
        if index >= 20 {
            return None;
        }
        sleep(Duration::from_millis(50)).await;
        Some((
            Ok::<_, std::convert::Infallible>(format!("event: message_delta\ndata: {{\"index\":{index}}}\n\n")),
            index + 1,
        ))
    }));

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, HeaderValue::from_static("text/event-stream"))
        .body(body)
        .expect("response")
}

async fn start_sse_server() -> (String, tokio::task::JoinHandle<()>) {
    let app = Router::new()
        .route("/json", get(slow_sse_handler))
        .route("/anthropic", get(slow_anthropic_sse_handler));
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let base_url = format!("http://{}", listener.local_addr().expect("addr"));
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.expect("serve");
    });
    (base_url, server)
}

#[tokio::test]
async fn for_each_sse_json_event_stops_early_on_abort() {
    let (base_url, server) = start_sse_server().await;
    let client = build_http_client(Some(5_000)).expect("client");
    let response = client.get(format!("{base_url}/json")).send().await.expect("request");

    let token = CancellationToken::new();
    let received = Arc::new(AtomicUsize::new(0));
    let received_for_task = received.clone();
    let token_for_task = token.clone();

    let task = tokio::spawn(async move {
        for_each_sse_json_event(response, &Some(token_for_task), |_event| {
            received_for_task.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })
        .await
    });

    sleep(Duration::from_millis(120)).await;
    token.cancel();

    let result = task.await.expect("join");
    let error = result.expect_err("aborted");
    assert_eq!(error.to_string(), REQUEST_ABORTED);
    let count = received.load(Ordering::SeqCst);
    assert!(count > 0, "expected at least one event before abort");
    assert!(count < 20, "expected abort to stop before all events, got {count}");

    server.abort();
}

#[tokio::test]
async fn for_each_anthropic_sse_event_stops_early_on_abort() {
    let (base_url, server) = start_sse_server().await;
    let client = build_http_client(Some(5_000)).expect("client");
    let response = client
        .get(format!("{base_url}/anthropic"))
        .send()
        .await
        .expect("request");

    let token = CancellationToken::new();
    let received = Arc::new(AtomicUsize::new(0));
    let received_for_task = received.clone();
    let token_for_task = token.clone();

    let task = tokio::spawn(async move {
        for_each_anthropic_sse_event(response, &Some(token_for_task), |_event| {
            received_for_task.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })
        .await
    });

    sleep(Duration::from_millis(120)).await;
    token.cancel();

    let result = task.await.expect("join");
    let error = result.expect_err("aborted");
    assert_eq!(error.to_string(), REQUEST_ABORTED);
    let count = received.load(Ordering::SeqCst);
    assert!(count > 0, "expected at least one event before abort");
    assert!(count < 20, "expected abort to stop before all events, got {count}");

    server.abort();
}
