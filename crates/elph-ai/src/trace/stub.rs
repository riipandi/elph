use std::future::Future;

use reqwest::RequestBuilder;

use crate::types::Model;

pub fn with_trace_headers(request: RequestBuilder) -> RequestBuilder {
    request
}

pub fn spawn_stream<F>(_model: &Model, fut: F) -> tokio::task::JoinHandle<()>
where
    F: Future<Output = ()> + Send + 'static,
{
    tokio::spawn(fut)
}
