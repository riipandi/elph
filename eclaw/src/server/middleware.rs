use std::time::{Duration, Instant};

use axum::{
    body::Body,
    http::{Method, Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Redirect, Response},
};

pub async fn request_logger(request: Request<Body>, next: Next) -> Response {
    let method = request.method().clone();
    let path = request.uri().path().to_owned();
    let query = request.uri().query().unwrap_or("").to_owned();
    let start = Instant::now();
    let response = next.run(request).await;
    log_request(&method, &path, &query, response.status(), start.elapsed());
    response
}

fn request_outcome(status: u16) -> &'static str {
    match status {
        500.. => "failed",
        400..=499 => "rejected",
        _ => "completed",
    }
}

fn log_request(method: &Method, path: &str, query: &str, status: StatusCode, latency: Duration) {
    let status_code = status.as_u16();
    let latency_ms = latency.as_millis() as u64;
    let outcome = request_outcome(status_code);

    match status_code {
        500.. => tracing::error!(
            %method,
            path,
            query,
            status = status_code,
            latency_ms,
            outcome,
            "request failed"
        ),
        400..=499 => tracing::warn!(
            %method,
            path,
            query,
            status = status_code,
            latency_ms,
            outcome,
            "request rejected"
        ),
        _ => tracing::info!(
            %method,
            path,
            query,
            status = status_code,
            latency_ms,
            outcome,
            "request completed"
        ),
    }
}

pub async fn trim_trailing_slash(request: Request<Body>, next: Next) -> Response {
    let path = request.uri().path();
    if path.len() > 1 && path.ends_with('/') {
        let trimmed = &path[..path.len() - 1];
        let location = match request.uri().query() {
            Some(query) => format!("{trimmed}?{query}"),
            None => trimmed.to_string(),
        };
        return Redirect::permanent(&location).into_response();
    }

    next.run(request).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_status_to_outcome() {
        assert_eq!(request_outcome(200), "completed");
        assert_eq!(request_outcome(404), "rejected");
        assert_eq!(request_outcome(500), "failed");
    }
}
