//! OpenAI Codex Responses API (ChatGPT backend).

use std::collections::HashSet;

use anyhow::{Result, anyhow};

use serde_json::{Value, json};

use crate::api::codex_transport::{
    CodexTransport, CodexTransportOptions, collect_codex_events_detailed, update_codex_websocket_continuation,
};
use crate::api::common::{
    apply_on_payload, build_http_client_for_target, finish_stream_error, is_request_aborted, merge_model_headers,
};
use crate::api::openai_prompt_cache::clamp_openai_prompt_cache_key;
use crate::api::openai_responses_shared::{
    ConvertResponsesMessagesOptions, convert_responses_messages, convert_responses_tools, process_responses_stream,
};
use crate::api::simple_options::build_base_options;
use crate::models::{clamp_thinking_level, thinking_level_to_str};
use crate::types::{
    AssistantMessage, AssistantMessageEvent, Context, Message, Model, ProviderStreams, SimpleStreamOptions, StopReason,
    StreamOptions, Usage,
};
use crate::utils::event_stream::AssistantMessageEventStream;

const DEFAULT_CODEX_BASE_URL: &str = "https://chatgpt.com/backend-api";
const CODEX_TOOL_CALL_PROVIDERS: &[&str] = &["openai", "openai-codex", "opencode"];

#[derive(Clone, Default)]
pub struct OpenAICodexResponsesOptions {
    pub base: StreamOptions,
    pub reasoning_effort: Option<String>,
    pub reasoning_summary: Option<String>,
    pub service_tier: Option<String>,
    pub text_verbosity: Option<String>,
    pub transport: CodexTransport,
    pub websocket_connect_timeout_ms: Option<u64>,
}

pub struct OpenAICodexResponsesApi;

impl ProviderStreams for OpenAICodexResponsesApi {
    fn stream(&self, model: &Model, context: &Context, options: Option<StreamOptions>) -> AssistantMessageEventStream {
        self.stream_with_options(
            model,
            context,
            OpenAICodexResponsesOptions {
                base: options.unwrap_or_default(),
                ..Default::default()
            },
        )
    }

    fn stream_simple(
        &self,
        model: &Model,
        context: &Context,
        options: Option<SimpleStreamOptions>,
    ) -> AssistantMessageEventStream {
        let opts = options.as_ref();
        let base = build_base_options(model, context, opts, opts.and_then(|o| o.base.api_key.clone()));
        let reasoning = opts.and_then(|o| o.reasoning).map(|r| clamp_thinking_level(model, r));
        let reasoning_effort = reasoning.map(|r| thinking_level_to_str(r).to_string());
        self.stream_with_options(
            model,
            context,
            OpenAICodexResponsesOptions {
                base,
                reasoning_effort,
                ..Default::default()
            },
        )
    }
}

impl OpenAICodexResponsesApi {
    pub fn stream_with_options(
        &self,
        model: &Model,
        context: &Context,
        options: OpenAICodexResponsesOptions,
    ) -> AssistantMessageEventStream {
        let stream = AssistantMessageEventStream::new();
        let model = model.clone();
        let context = context.clone();
        let s = stream.clone();
        tokio::spawn(async move {
            let mut output = AssistantMessage::empty(&model);
            if let Err(e) = run_codex(&model, &context, &options, &s, &mut output).await {
                let aborted = crate::api::common::is_abort_error(&e);
                finish_stream_error(&s, &mut output, e, aborted);
            }
        });
        stream
    }
}

async fn run_codex(
    model: &Model,
    context: &Context,
    options: &OpenAICodexResponsesOptions,
    stream: &AssistantMessageEventStream,
    output: &mut AssistantMessage,
) -> Result<()> {
    let api_key = options
        .base
        .api_key
        .as_deref()
        .ok_or_else(|| anyhow!("No API key for provider: {}", model.provider))?;
    let account_id = extract_account_id(api_key)?;
    let providers: HashSet<String> = CODEX_TOOL_CALL_PROVIDERS.iter().map(|s| s.to_string()).collect();
    let mut body = build_request_body(model, context, options, &providers)?;
    body = apply_on_payload(options.base.on_payload.as_ref(), body, model).await;

    let mut headers = merge_model_headers(model, Some(&options.base));
    headers.insert("Authorization".to_string(), format!("Bearer {api_key}"));
    headers.insert("chatgpt-account-id".to_string(), account_id);
    headers.insert("originator".to_string(), "elph".to_string());
    headers.insert("OpenAI-Beta".to_string(), "responses=experimental".to_string());
    headers.insert("accept".to_string(), "text/event-stream".to_string());
    if let Some(sid) = &options.base.session_id {
        headers.insert("session-id".to_string(), sid.clone());
        headers.insert("x-client-request-id".to_string(), sid.clone());
    }

    let url = resolve_codex_url(&model.base_url);
    let client = build_http_client_for_target(options.base.timeout_ms, Some(&url), options.base.env.as_ref())?;

    stream.push(AssistantMessageEvent::Start {
        partial: output.clone(),
    });
    let full_body = body.clone();
    let collect = collect_codex_events_detailed(
        &model.base_url,
        body,
        headers.clone(),
        &client,
        &url,
        &CodexTransportOptions {
            transport: options.transport.clone(),
            websocket_connect_timeout_ms: options.websocket_connect_timeout_ms,
            session_id: options.base.session_id.clone(),
            signal: options.base.signal.clone(),
            env: options.base.env.clone(),
        },
    )
    .await?;
    let raw_events: Result<Vec<Value>> = collect.events.into_iter().map(map_codex_event).collect();
    let raw_events = raw_events?;
    let service_tier = options.service_tier.clone();
    let model_id = model.id.clone();
    process_responses_stream(
        raw_events,
        output,
        stream,
        model,
        Some(crate::api::openai_responses_shared::OpenAIResponsesStreamOptions {
            service_tier,
            resolve_service_tier: None,
            apply_service_tier_pricing: Some(Box::new(move |usage, tier| {
                apply_service_tier_pricing(usage, tier, &model_id);
            })),
        }),
    )
    .await?;

    if collect.used_cached_context
        && let (Some(session_id), Some(response_id)) =
            (options.base.session_id.as_deref(), output.response_id.as_deref())
    {
        let response_items = response_items_from_output(model, output, &providers);
        update_codex_websocket_continuation(session_id, &full_body, response_id, json!(response_items));
    }

    if is_request_aborted(&options.base.signal) {
        output.stop_reason = StopReason::Aborted;
    }
    stream.push(AssistantMessageEvent::Done {
        reason: output.stop_reason,
        message: output.clone(),
    });
    stream.end();
    Ok(())
}

fn map_codex_event(mut event: Value) -> Result<Value> {
    let event_type = event.get("type").and_then(|v| v.as_str()).unwrap_or("");
    match event_type {
        "error" => {
            let msg = event.get("message").and_then(|v| v.as_str()).unwrap_or("Codex error");
            return Err(anyhow!("{msg}"));
        }
        "response.done" | "response.completed" | "response.incomplete" => {
            if let Some(response) = event.get_mut("response")
                && let Some(status) = response.get("status").and_then(|v| v.as_str())
            {
                response["status"] = json!(normalize_codex_status(status));
            }
            event["type"] = json!("response.completed");
        }
        _ => {}
    }
    Ok(event)
}

fn normalize_codex_status(status: &str) -> &str {
    match status {
        "completed" | "incomplete" | "failed" | "cancelled" | "queued" | "in_progress" => status,
        _ => "completed",
    }
}

fn build_request_body(
    model: &Model,
    context: &Context,
    options: &OpenAICodexResponsesOptions,
    providers: &HashSet<String>,
) -> Result<Value> {
    let messages = convert_responses_messages(
        model,
        context,
        providers,
        Some(ConvertResponsesMessagesOptions {
            include_system_prompt: false,
        }),
    );
    let mut body = json!({
        "model": model.id,
        "store": false,
        "stream": true,
        "instructions": context.system_prompt.clone().unwrap_or_else(|| "You are a helpful assistant.".to_string()),
        "input": messages,
        "text": { "verbosity": options.text_verbosity.clone().unwrap_or_else(|| "low".to_string()) },
        "include": ["reasoning.encrypted_content"],
        "prompt_cache_key": clamp_openai_prompt_cache_key(options.base.session_id.as_deref()),
        "tool_choice": "auto",
        "parallel_tool_calls": true
    });
    if let Some(tools) = &context.tools
        && !tools.is_empty()
    {
        body["tools"] = json!(convert_responses_tools(tools, Some(false)));
    }
    if let Some(effort) = &options.reasoning_effort {
        body["reasoning"] = json!({
            "effort": effort,
            "summary": options.reasoning_summary.clone().unwrap_or_else(|| "auto".to_string())
        });
    }
    Ok(body)
}

fn extract_account_id(token: &str) -> Result<String> {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return Err(anyhow!("Failed to extract accountId from token"));
    }
    use base64::Engine;
    let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(parts[1])
        .or_else(|_| base64::engine::general_purpose::STANDARD.decode(parts[1]))?;
    let json: Value = serde_json::from_slice(&payload)?;
    json.pointer("/https://api.openai.com/auth/chatgpt_account_id")
        .or_else(|| json.get("chatgpt_account_id"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| anyhow!("No account ID in token"))
}

fn resolve_codex_url(base_url: &str) -> String {
    let raw = if base_url.trim().is_empty() {
        DEFAULT_CODEX_BASE_URL
    } else {
        base_url
    };
    let normalized = raw.trim().trim_end_matches('/');
    if normalized.ends_with("/codex/responses") {
        normalized.to_string()
    } else if normalized.ends_with("/codex") {
        format!("{normalized}/responses")
    } else {
        format!("{normalized}/codex/responses")
    }
}

fn response_items_from_output(model: &Model, output: &AssistantMessage, providers: &HashSet<String>) -> Vec<Value> {
    let context = Context {
        system_prompt: None,
        messages: vec![Message::Assistant(output.clone())],
        tools: None,
    };
    convert_responses_messages(
        model,
        &context,
        providers,
        Some(ConvertResponsesMessagesOptions {
            include_system_prompt: false,
        }),
    )
    .into_iter()
    .filter(|item| item.get("type").and_then(|v| v.as_str()) != Some("function_call_output"))
    .collect()
}

fn apply_service_tier_pricing(usage: &mut Usage, service_tier: Option<&str>, model_id: &str) {
    let multiplier = match service_tier {
        Some("flex") => 0.5,
        Some("priority") if model_id == "gpt-5.5" => 2.5,
        Some("priority") => 2.0,
        _ => 1.0,
    };
    if multiplier == 1.0 {
        return;
    }
    usage.cost.input *= multiplier;
    usage.cost.output *= multiplier;
    usage.cost.cache_read *= multiplier;
    usage.cost.cache_write *= multiplier;
    usage.cost.total = usage.cost.input + usage.cost.output + usage.cost.cache_read + usage.cost.cache_write;
}
