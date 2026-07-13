use reqwest::RequestBuilder;

pub fn with_trace_headers(request: RequestBuilder) -> RequestBuilder {
    request.headers(fastrace_reqwest::traceparent_headers())
}
