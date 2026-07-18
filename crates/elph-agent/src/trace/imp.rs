use elph_core::trace;
use reqwest::RequestBuilder;

pub fn with_trace_headers(request: RequestBuilder) -> RequestBuilder {
    if !trace::is_enabled() {
        return request;
    }
    request.headers(fastrace_reqwest::traceparent_headers())
}
