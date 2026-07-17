use std::future::Future;
use std::path::Path;

use reqwest::RequestBuilder;

use crate::types::Model;

/// No-op root span guard when the `tracing` feature is disabled.
pub struct RootSpanGuard;

/// Runtime tracing is never active without the `tracing` feature.
pub fn is_enabled() -> bool {
    false
}

/// Initialize tracing. No-op without the `tracing` feature.
pub fn init(_logs_dir: &Path, _app_name: &str, _enabled: bool) {}

/// Flush pending spans. No-op without the `tracing` feature.
pub fn flush() {}

/// Start a root span. No-op without the `tracing` feature.
pub fn root_span(_name: &'static str) -> RootSpanGuard {
    RootSpanGuard
}

pub fn with_trace_headers(request: RequestBuilder) -> RequestBuilder {
    request
}

pub fn spawn_stream<F>(_model: &Model, fut: F) -> tokio::task::JoinHandle<()>
where
    F: Future<Output = ()> + Send + 'static,
{
    tokio::spawn(fut)
}
