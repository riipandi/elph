use std::collections::HashSet;

use anyhow::Result;

use serde_json::{Value, json};

use crate::api::common::{
    apply_on_payload, build_http_client, finish_stream_error, get_client_api_key, invoke_on_response_from_reqwest,
    merge_model_headers,
};
use crate::api::github_copilot_headers::{build_copilot_dynamic_headers, has_copilot_vision_input};
use crate::api::openai_prompt_cache::clamp_openai_prompt_cache_key;
use crate::api::openai_responses_shared::{
    OpenAIResponsesStreamOptions, convert_responses_messages, convert_responses_tools, process_responses_stream,
};
use crate::api::simple_options::build_base_options;
use crate::models::{clamp_thinking_level, thinking_level_to_str};
use crate::types::{
    AssistantMessage, AssistantMessageEvent, Context, Model, ProviderStreams, SimpleStreamOptions, StreamOptions, Usage,
};
use crate::utils::event_stream::AssistantMessageEventStream;
use crate::utils::provider_env::get_provider_env_value;

use super::sse::collect_sse_json_events;

const OPENAI_TOOL_CALL_PROVIDERS: &[&str] = &["openai", "openai-codex", "opencode"];
const OPENAI_RESPONSES_MIN_OUTPUT_TOKENS: u32 = 16;

#[derive(Clone, Default)]
pub struct OpenAIResponsesOptions {
    pub base: StreamOptions,
    pub reasoning_effort: Option<String>,
    pub reasoning_summary: Option<String>,
    pub service_tier: Option<String>,
}

pub struct OpenAIResponsesApi;

impl ProviderStreams for OpenAIResponsesApi {
    fn stream(&self, model: &Model, context: &Context, options: Option<StreamOptions>) -> AssistantMessageEventStream {
        self.stream_with_options(
            model,
            context,
            OpenAIResponsesOptions {
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
            OpenAIResponsesOptions {
                base,
                reasoning_effort,
                ..Default::default()
            },
        )
    }
}

impl OpenAIResponsesApi {
    pub fn stream_with_options(
        &self,
        model: &Model,
        context: &Context,
        options: OpenAIResponsesOptions,
    ) -> AssistantMessageEventStream {
        let stream = AssistantMessageEventStream::new();
        let model = model.clone();
        let context = context.clone();
        let s = stream.clone();
        tokio::spawn(async move {
            let mut output = AssistantMessage::empty(&model);
            if let Err(e) = run_openai_responses(&model, &context, &options, &s, &mut output).await {
                let aborted = crate::api::common::is_abort_error(&e);
                finish_stream_error(&s, &mut output, e, aborted);
            }
        });
        stream
    }
}

async fn run_openai_responses(
    model: &Model,
    context: &Context,
    options: &OpenAIResponsesOptions,
    stream: &AssistantMessageEventStream,
    output: &mut AssistantMessage,
) -> Result<()> {
    let mut headers = merge_model_headers(model, Some(&options.base));
    if model.provider == "github-copilot" {
        headers.extend(build_copilot_dynamic_headers(
            &context.messages,
            has_copilot_vision_input(&context.messages),
        ));
    }
    let api_key = get_client_api_key(&model.provider, options.base.api_key.as_deref(), &headers)?;
    let providers: HashSet<String> = OPENAI_TOOL_CALL_PROVIDERS.iter().map(|s| s.to_string()).collect();
    let mut params = build_params(model, context, options, &providers)?;
    params = apply_on_payload(options.base.on_payload.as_ref(), params, model).await;

    let client = build_http_client(options.base.timeout_ms)?;
    let url = format!("{}/responses", model.base_url.trim_end_matches('/'));
    let mut req = client.post(&url).bearer_auth(&api_key).json(&params);
    for (k, v) in &headers {
        req = req.header(k, v);
    }
    let response = crate::api::common::send_with_abort(&options.base.signal, req).await?;
    invoke_on_response_from_reqwest(options.base.on_response.as_ref(), &response, model).await;
    let response = crate::api::common::check_response_ok(response).await?;

    stream.push(AssistantMessageEvent::Start {
        partial: output.clone(),
    });
    let events = collect_sse_json_events(response).await?;

    let service_tier = options.service_tier.clone();
    let model_id = model.id.clone();
    process_responses_stream(
        events,
        output,
        stream,
        model,
        Some(OpenAIResponsesStreamOptions {
            service_tier: service_tier.clone(),
            resolve_service_tier: None,
            apply_service_tier_pricing: Some(Box::new(move |usage, tier| {
                apply_service_tier_pricing(usage, tier, &model_id);
            })),
        }),
    )
    .await?;

    stream.push(AssistantMessageEvent::Done {
        reason: output.stop_reason,
        message: output.clone(),
    });
    stream.end();
    Ok(())
}

/// Build OpenAI Responses request params (used by integration tests mirroring pi-ai).
pub fn build_openai_responses_params(
    model: &Model,
    context: &Context,
    options: &OpenAIResponsesOptions,
) -> Result<Value> {
    let providers: HashSet<String> = OPENAI_TOOL_CALL_PROVIDERS.iter().map(|s| s.to_string()).collect();
    build_params(model, context, options, &providers)
}

fn build_params(
    model: &Model,
    context: &Context,
    options: &OpenAIResponsesOptions,
    providers: &HashSet<String>,
) -> Result<Value> {
    let messages = convert_responses_messages(model, context, providers, None);
    let cache_retention = resolve_cache_retention(&options.base);
    let mut params = json!({
        "model": model.id,
        "input": messages,
        "stream": true,
        "store": false
    });
    if cache_retention != crate::types::CacheRetention::None {
        let supports_long = model
            .openai_responses_compat
            .as_ref()
            .and_then(|c| c.supports_long_cache_retention)
            .unwrap_or(true);
        let is_openai_host = model.base_url.contains("api.openai.com");
        let allow_cache = cache_retention != crate::types::CacheRetention::Long || supports_long || !is_openai_host;
        if allow_cache {
            if let Some(key) = clamp_openai_prompt_cache_key(options.base.session_id.as_deref()) {
                params["prompt_cache_key"] = json!(key);
            }
            if cache_retention == crate::types::CacheRetention::Long {
                params["prompt_cache_retention"] = json!("24h");
            }
        }
    }
    if let Some(max) = options.base.max_tokens {
        params["max_output_tokens"] = json!(max.max(OPENAI_RESPONSES_MIN_OUTPUT_TOKENS));
    }
    if let Some(temp) = options.base.temperature {
        params["temperature"] = json!(temp);
    }
    if let Some(tools) = &context.tools
        && !tools.is_empty()
    {
        params["tools"] = json!(convert_responses_tools(tools, None));
    }
    if model.reasoning
        && let Some(effort) = &options.reasoning_effort
    {
        params["reasoning"] = json!({ "effort": effort, "summary": options.reasoning_summary.clone().unwrap_or_else(|| "auto".to_string()) });
        params["include"] = json!(["reasoning.encrypted_content"]);
    }
    Ok(params)
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

fn resolve_cache_retention(options: &StreamOptions) -> crate::types::CacheRetention {
    if let Some(r) = options.cache_retention {
        return r;
    }
    if get_provider_env_value("PI_CACHE_RETENTION", options.env.as_ref()) == Some("long".to_string()) {
        crate::types::CacheRetention::Long
    } else {
        crate::types::CacheRetention::Short
    }
}
