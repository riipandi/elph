use reqwest::RequestBuilder;

/// No-op root span guard when the `tracing` feature is disabled.
pub struct RootSpanGuard;

/// Runtime tracing is never active without the `tracing` feature.
pub fn is_enabled() -> bool {
    false
}

/// Flush pending spans. No-op without the `tracing` feature.
pub fn flush() {}

/// Start a root span. No-op without the `tracing` feature.
pub fn root_span(_name: &'static str) -> RootSpanGuard {
    RootSpanGuard
}

pub fn with_trace_headers(request: RequestBuilder) -> RequestBuilder {
    request
}
