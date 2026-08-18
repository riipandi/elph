use fastrace::prelude::{Event, LocalSpan};
use reqwest::RequestBuilder;

use super as trace;

pub fn with_trace_headers(request: RequestBuilder) -> RequestBuilder {
    if !trace::is_enabled() {
        return request;
    }
    request.headers(fastrace_reqwest::traceparent_headers())
}

pub fn add_property(key: &'static str, value: impl Into<String>) {
    if !trace::is_enabled() {
        return;
    }
    let value = value.into();
    LocalSpan::add_property(move || (key, value));
}

pub fn add_event(name: &'static str) {
    if !trace::is_enabled() {
        return;
    }
    LocalSpan::add_event(Event::new(name));
}

// model_stream_span / spawn_stream are AI-side; agent only needs headers.
// Keep stubs for API stability if referenced.
use fastrace::collector::SpanContext;
use fastrace::future::FutureExt;
use fastrace::prelude::Span;
use std::future::Future;

pub fn model_stream_span(name: &'static str) -> Span {
    Span::root(name, SpanContext::random())
}

pub fn spawn_stream<F>(name: &'static str, fut: F) -> tokio::task::JoinHandle<()>
where
    F: Future<Output = ()> + Send + 'static,
{
    if !trace::is_enabled() {
        return tokio::spawn(fut);
    }
    tokio::spawn(fut.in_span(model_stream_span(name)))
}
