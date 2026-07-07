use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use anyhow::{Result, anyhow};
use reqwest::Client;
use serde_json::Value;

use crate::types::{
    AssistantMessage, AssistantMessageEvent, Model, OnPayloadCallback, OnResponseCallback, ProviderResponse,
    StopReason, StreamOptions,
};
use crate::utils::error_body::{error_body_from_response, format_provider_error, normalize_provider_error};
use crate::utils::event_stream::AssistantMessageEventStream;
use crate::utils::headers::{has_header, headers_to_record, merge_provider_headers};

pub fn build_http_client(timeout_ms: Option<u64>) -> Result<Client> {
    let mut builder = Client::builder();
    if let Some(ms) = timeout_ms {
        builder = builder.timeout(std::time::Duration::from_millis(ms));
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

pub async fn send_with_abort(
    token: &Option<tokio_util::sync::CancellationToken>,
    request: reqwest::RequestBuilder,
) -> Result<reqwest::Response> {
    if is_request_aborted(token) {
        return Err(request_aborted_error());
    }
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
