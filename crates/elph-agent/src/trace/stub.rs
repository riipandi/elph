use reqwest::RequestBuilder;

pub fn with_trace_headers(request: RequestBuilder) -> RequestBuilder {
    request
}

pub fn add_property(_key: &'static str, _value: impl Into<String>) {}

pub fn add_event(_name: &'static str) {}

pub fn model_stream_span(_name: &'static str) {}

pub fn spawn_stream<F>(_name: &'static str, fut: F) -> tokio::task::JoinHandle<()>
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    tokio::spawn(fut)
}
