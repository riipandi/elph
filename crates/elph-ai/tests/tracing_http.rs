#![cfg(feature = "tracing")]

use elph_ai::trace::with_trace_headers;
use elph_core::trace::JsonlReporter;
use elph_core::trace::set_reporter;
use fastrace::collector::{Config, SpanContext};
use fastrace::prelude::Span;

#[tokio::test]
async fn with_trace_headers_injects_traceparent() {
    let dir = tempfile::tempdir().expect("tempdir");
    let reporter = JsonlReporter::new(dir.path(), "elph").expect("reporter");
    set_reporter(reporter, Config::default());

    let span = Span::root("elph.test.http", SpanContext::random());
    let _guard = span.set_local_parent();

    let client = reqwest::Client::new();
    let request = with_trace_headers(client.get("https://example.com"));
    let built = request.build().expect("request");
    let traceparent = built
        .headers()
        .get("traceparent")
        .expect("traceparent header")
        .to_str()
        .expect("header value");
    assert!(traceparent.starts_with("00-"));
}
