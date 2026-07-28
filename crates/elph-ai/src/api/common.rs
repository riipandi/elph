use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use anyhow::Result;
use anyhow::anyhow;
use reqwest::Client;
use serde_json::Value;

use crate::api::http_proxy::resolve_http_proxy_url_for_target;
use crate::resilience::ResilienceError;
use crate::types::{AssistantMessage, AssistantMessageEvent, Model, OnPayloadCallback, OnResponseCallback};
use crate::types::{ProviderEnv, ProviderResponse, StopReason, StreamOptions};
use crate::utils::error_body::{error_body_from_response, format_provider_error, normalize_provider_error};
use crate::utils::event_stream::AssistantMessageEventStream;
use crate::utils::headers::{has_header, headers_to_record, merge_provider_headers};

pub fn build_http_client(timeout_ms: Option<u64>) -> Result<Client> {
    build_http_client_for_target(timeout_ms, None, None)
}

pub fn build_http_client_for_target(
    timeout_ms: Option<u64>,
    target_url: Option<&str>,
    env: Option<&ProviderEnv>,
) -> Result<Client> {
    let mut builder = Client::builder();
    if let Some(ms) = timeout_ms {
        builder = builder.timeout(std::time::Duration::from_millis(ms));
    }
    if let Some(target_url) = target_url
        && let Some(proxy_url) = resolve_http_proxy_url_for_target(target_url, env)?
    {
        let proxy = reqwest::Proxy::all(proxy_url.as_str())?;
        builder = builder.proxy(proxy);
    }
    Ok(builder.build()?)
}

pub fn get_client_api_key(provider: &str, api_key: Option<&str>, headers: &HashMap<String, String>) -> Result<String> {
    if let Some(key) = api_key {
        return Ok(key.to_string());
    }
    if has_header(headers, "authorization") || has_header(headers, "cf-aig-authorization") {
        return Ok("unused".to_string());
    }
    Err(anyhow!("No API key for provider: {provider}"))
}

pub async fn apply_on_payload(callback: Option<&OnPayloadCallback>, payload: Value, model: &Model) -> Value {
    if let Some(cb) = callback {
        let m = model.clone();
        let original = payload.clone();
        if let Some(next) = cb(payload, m).await {
            return next;
        }
        return original;
    }
    payload
}

pub async fn apply_on_response(callback: Option<&OnResponseCallback>, response: ProviderResponse, model: &Model) {
    if let Some(cb) = callback {
        let m = model.clone();
        cb(response, m).await;
    }
}

pub fn merge_model_headers(model: &Model, options: Option<&StreamOptions>) -> HashMap<String, String> {
    let base = model.headers.clone().unwrap_or_default();
    merge_provider_headers(&base, options.and_then(|o| o.headers.as_ref()))
}

pub const REQUEST_ABORTED: &str = "Request aborted";

pub fn is_request_aborted(token: &Option<tokio_util::sync::CancellationToken>) -> bool {
    token.as_ref().is_some_and(|t| t.is_cancelled())
}

pub fn request_aborted_error() -> anyhow::Error {
    anyhow!(REQUEST_ABORTED)
}

pub fn is_abort_error(error: &anyhow::Error) -> bool {
    error.to_string() == REQUEST_ABORTED
}

pub fn with_trace_headers(request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
    crate::trace::with_trace_headers(request)
}

pub async fn send_with_abort(
    token: &Option<tokio_util::sync::CancellationToken>,
    request: reqwest::RequestBuilder,
) -> Result<reqwest::Response> {
    if is_request_aborted(token) {
        return Err(request_aborted_error());
    }
    let request = with_trace_headers(request);
    match token {
        Some(token) => {
            let token = token.clone();
            tokio::select! {
                result = request.send() => result.map_err(Into::into),
                _ = token.cancelled() => Err(request_aborted_error()),
            }
        }
        None => request.send().await.map_err(Into::into),
    }
}

pub fn finish_stream_error(
    stream: &AssistantMessageEventStream,
    output: &mut AssistantMessage,
    error: anyhow::Error,
    aborted: bool,
) {
    output.stop_reason = if aborted {
        StopReason::Aborted
    } else {
        StopReason::Error
    };
    output.error_message = Some(format_provider_error(&normalize_provider_error(&error), None));
    stream.push(AssistantMessageEvent::Error {
        reason: output.stop_reason,
        error: output.clone(),
    });
    stream.end();
}

pub async fn check_response_ok(response: reqwest::Response) -> Result<reqwest::Response> {
    if response.status().is_success() {
        return Ok(response);
    }
    let status = response.status();
    let body = error_body_from_response(response).await;
    Err(anyhow!("{status}: {body}"))
}

pub type StreamTask = Pin<Box<dyn Future<Output = ()> + Send>>;

pub fn spawn_stream_task(fut: impl Future<Output = ()> + Send + 'static) -> StreamTask {
    Box::pin(async move {
        tokio::spawn(fut);
    })
}

pub fn wrap_on_payload<F>(f: F) -> OnPayloadCallback
where
    F: Fn(Value, Model) -> Pin<Box<dyn Future<Output = Option<Value>> + Send>> + Send + Sync + 'static,
{
    Arc::new(f)
}

pub fn wrap_on_response<F>(f: F) -> OnResponseCallback
where
    F: Fn(ProviderResponse, Model) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync + 'static,
{
    Arc::new(f)
}

pub async fn invoke_on_response_from_reqwest(
    callback: Option<&OnResponseCallback>,
    response: &reqwest::Response,
    model: &Model,
) {
    let provider_response = ProviderResponse {
        status: response.status().as_u16(),
        headers: headers_to_record(response.headers()),
    };
    apply_on_response(callback, provider_response, model).await;
}

// ---------------------------------------------------------------------------
// Resilience: rate limiter, circuit breaker, retry
// ---------------------------------------------------------------------------

/// Check rate limiter and circuit breaker before sending a request to a provider.
///
/// Returns `Ok(())` if the request can proceed.
/// Returns `Err` with a descriptive message if blocked.
pub fn check_provider_resilience(provider_id: &str) -> Result<()> {
    crate::resilience::check_provider_resilience(provider_id).map_err(|e| match e {
        ResilienceError::RateLimited => anyhow!("rate limited — too many requests to {provider_id}"),
        ResilienceError::CircuitOpen => anyhow!("circuit breaker open — {provider_id} is failing"),
    })
}

/// Record a successful call to a provider.
pub fn record_provider_success(provider_id: &str) {
    crate::resilience::record_provider_success(provider_id);
}

/// Record a failed call to a provider.
pub fn record_provider_failure(provider_id: &str) {
    crate::resilience::record_provider_failure(provider_id);
}

/// Send with abort + resilience checks.
///
/// Combines `check_provider_resilience()` with `send_with_abort()`.
/// Records success/failure in the circuit breaker automatically.
/// Abort errors are not counted as provider failures.
pub async fn send_with_resilience(
    provider_id: &str,
    token: &Option<tokio_util::sync::CancellationToken>,
    request: reqwest::RequestBuilder,
) -> Result<reqwest::Response> {
    check_provider_resilience(provider_id)?;
    match send_with_abort(token, request).await {
        Ok(response) => {
            // Record success for 2xx; non-2xx handled by caller
            if response.status().is_success() {
                record_provider_success(provider_id);
            }
            Ok(response)
        }
        Err(e) => {
            // Don't count abort errors as provider failures
            if !is_abort_error(&e) {
                record_provider_failure(provider_id);
            }
            Err(e)
        }
    }
}

/// Record resilience outcome based on an HTTP response status.
///
/// Call this after processing the response to update the circuit breaker.
/// 2xx = success, 429/5xx = failure.
pub fn record_resilience_from_status(provider_id: &str, status: u16) {
    if status == 429 || status >= 500 {
        record_provider_failure(provider_id);
    } else if (200..300).contains(&status) {
        record_provider_success(provider_id);
    }
}

/// Send with resilience + automatic retry on transient failures.
///
/// Builds the request, then retries with exponential backoff on:
/// - 429, 408, 409 (client errors that are retryable)
/// - 5xx (server errors)
/// - Connection/timeout errors
/// - Body transport/decoding errors (e.g. "error decoding response body")
///
/// For non-2xx responses, the error body is read inside the retry loop so
/// transport errors during body consumption also trigger retries.
///
/// Abort errors are not retried.
pub async fn send_with_resilience_retry(
    provider_id: &str,
    token: &Option<tokio_util::sync::CancellationToken>,
    client: &reqwest::Client,
    request: reqwest::RequestBuilder,
    max_retries: u32,
) -> Result<reqwest::Response> {
    use std::time::Duration;

    check_provider_resilience(provider_id)?;

    // Build the request so we can clone it for retries
    let built = request.build()?;

    let mut last_err: Option<anyhow::Error> = None;
    for attempt in 0..=max_retries {
        if attempt > 0 {
            // Exponential backoff: 500ms, 1s, 2s, 4s, ...
            let delay = Duration::from_millis(500 * 2u64.pow(attempt.saturating_sub(1)));
            tokio::time::sleep(delay).await;
            log::debug!("resilience: retrying {provider_id} (attempt {attempt}/{max_retries})");
        }

        // Check abort before each attempt
        if is_request_aborted(token) {
            return Err(request_aborted_error());
        }

        // Clone the request for this attempt
        let req_clone = match built.try_clone() {
            Some(r) => r,
            None => {
                // Non-cloneable body (stream) — can't retry
                return Err(anyhow!("cannot retry non-cloneable request body"));
            }
        };

        // Execute with abort support — client.execute() returns a future directly
        let result = match token {
            Some(token) => {
                let token = token.clone();
                tokio::select! {
                    result = client.execute(req_clone) => result.map_err(Into::into),
                    _ = token.cancelled() => Err(request_aborted_error()),
                }
            }
            None => client.execute(req_clone).await.map_err(Into::into),
        };

        match result {
            Ok(response) => {
                let status_code = response.status();

                // 2xx — success, body untouched for SSE streaming
                if status_code.is_success() {
                    record_provider_success(provider_id);
                    return Ok(response);
                }

                // Read the error body — this can fail with transport/decoding errors
                let body = match response.text().await {
                    Ok(text) => text,
                    Err(e) => {
                        // Transport error while reading body (e.g. "error decoding response body")
                        // This is retryable — the body was corrupted during transfer
                        record_provider_failure(provider_id);
                        last_err = Some(anyhow::Error::from(e));
                        continue;
                    }
                };

                let code = status_code.as_u16();

                // Determine if this error is retryable
                let is_retryable = code == 429
                    || code >= 500
                    || code == 408  // Request Timeout
                    || code == 409  // Conflict (sometimes transient)
                    || crate::resilience::retry::is_anyhow_retryable(&anyhow::anyhow!("{code}: {body}"));

                if is_retryable {
                    record_provider_failure(provider_id);
                    last_err = Some(anyhow!("HTTP {code}: {body}"));
                    continue;
                }

                // Non-retryable error — fail immediately
                record_provider_failure(provider_id);
                return Err(anyhow!("{code}: {body}"));
            }
            Err(e) => {
                if is_abort_error(&e) {
                    return Err(e);
                }
                // Connection/timeout error — retry
                record_provider_failure(provider_id);
                last_err = Some(e);
                continue;
            }
        }
    }

    // All retries exhausted
    Err(last_err.unwrap_or_else(|| anyhow!("max retries exhausted for {provider_id}")))
}
