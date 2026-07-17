use elph_ai::trace;
use reqwest::RequestBuilder;

pub use elph_ai::trace::{JsonlReporter, RootSpanGuard, flush, is_enabled, root_span, set_reporter};

pub fn with_trace_headers(request: RequestBuilder) -> RequestBuilder {
    if !trace::is_enabled() {
        return request;
    }
    request.headers(fastrace_reqwest::traceparent_headers())
}
