use reqwest::RequestBuilder;

pub fn with_trace_headers(request: RequestBuilder) -> RequestBuilder {
    request
}

pub fn model_stream_span(_name: &'static str) {}

pub fn spawn_stream<F>(_name: &'static str, fut: F) -> tokio::task::JoinHandle<()>
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    tokio::spawn(fut)
}
