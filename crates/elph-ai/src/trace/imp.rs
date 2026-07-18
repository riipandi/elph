use std::future::Future;

use elph_core::trace;
use fastrace::collector::SpanContext;
use fastrace::future::FutureExt;
use fastrace::prelude::Span;
use reqwest::RequestBuilder;

use crate::types::Model;

pub fn with_trace_headers(request: RequestBuilder) -> RequestBuilder {
    if !trace::is_enabled() {
        return request;
    }
    request.headers(fastrace_reqwest::traceparent_headers())
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
