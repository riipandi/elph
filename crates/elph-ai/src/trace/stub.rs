use std::future::Future;

use reqwest::RequestBuilder;

use crate::types::Model;

pub fn with_trace_headers(request: RequestBuilder) -> RequestBuilder {
    request
}

pub fn inject_traceparent(_request: &mut reqwest::Request) {}

pub fn add_property(_key: &'static str, _value: impl Into<String>) {}

pub fn add_event(_name: &'static str) {}

pub fn model_stream_span(_model: &Model) {}

pub fn spawn_stream<F>(_model: &Model, fut: F) -> tokio::task::JoinHandle<()>
where
    F: Future<Output = ()> + Send + 'static,
{
    tokio::spawn(fut)
}
