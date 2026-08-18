use std::future::Future;

use fastrace::collector::SpanContext;
use fastrace::future::FutureExt;
use fastrace::prelude::{Event, LocalSpan, Span};
use reqwest::RequestBuilder;

use super as trace;
use crate::types::Model;

pub fn with_trace_headers(request: RequestBuilder) -> RequestBuilder {
    if !trace::is_enabled() {
        return request;
    }
    request.headers(fastrace_reqwest::traceparent_headers())
}

/// Inject W3C `traceparent` onto an already-built request (retry / `Client::execute`).
pub fn inject_traceparent(request: &mut reqwest::Request) {
    if !trace::is_enabled() {
        return;
    }
    for (key, value) in fastrace_reqwest::traceparent_headers().iter() {
        request.headers_mut().insert(key, value.clone());
    }
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

pub fn model_stream_span(model: &Model) -> Span {
    let span = Span::root("elph.ai.stream", SpanContext::random());
    span.add_property(|| ("model.id", model.id.clone()));
    span.add_property(|| ("model.provider", model.provider.clone()));
    span.add_property(|| ("model.api", model.api.clone()));
    span
}

pub fn spawn_stream<F>(model: &Model, fut: F) -> tokio::task::JoinHandle<()>
where
    F: Future<Output = ()> + Send + 'static,
{
    if !trace::is_enabled() {
        return tokio::spawn(fut);
    }
    tokio::spawn(fut.in_span(model_stream_span(model)))
}
