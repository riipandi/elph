use std::collections::HashSet;

use anyhow::{Result, anyhow};

use serde_json::{Value, json};

use crate::api::common::{
    apply_on_payload, build_http_client, finish_stream_error, invoke_on_response_from_reqwest, merge_model_headers,
};
use crate::api::openai_prompt_cache::clamp_openai_prompt_cache_key;
use crate::api::openai_responses_shared::{
    convert_responses_messages, convert_responses_tools, process_responses_stream,
};
use crate::api::simple_options::build_base_options;
use crate::models::{clamp_thinking_level, thinking_level_to_str};
use crate::types::{
    AssistantMessage, AssistantMessageEvent, Context, Model, ProviderStreams, SimpleStreamOptions, StreamOptions,
};
use crate::utils::event_stream::AssistantMessageEventStream;
use crate::utils::provider_env::get_provider_env_value;

use super::sse::collect_sse_json_events;

const DEFAULT_AZURE_API_VERSION: &str = "v1";
const AZURE_TOOL_CALL_PROVIDERS: &[&str] = &["openai", "openai-codex", "opencode", "azure-openai-responses"];
const OPENAI_RESPONSES_MIN_OUTPUT_TOKENS: u32 = 16;

#[derive(Clone, Default)]
pub struct AzureOpenAIResponsesOptions {
    pub base: StreamOptions,
    pub reasoning_effort: Option<String>,
    pub reasoning_summary: Option<String>,
    pub azure_api_version: Option<String>,
    pub azure_resource_name: Option<String>,
    pub azure_base_url: Option<String>,
    pub azure_deployment_name: Option<String>,
}

pub struct AzureOpenAIResponsesApi;

impl ProviderStreams for AzureOpenAIResponsesApi {
    fn stream(&self, model: &Model, context: &Context, options: Option<StreamOptions>) -> AssistantMessageEventStream {
        self.stream_with_options(
            model,
            context,
            AzureOpenAIResponsesOptions {
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
            AzureOpenAIResponsesOptions {
                base,
                reasoning_effort,
                ..Default::default()
            },
        )
    }
}

impl AzureOpenAIResponsesApi {
    pub fn stream_with_options(
        &self,
        model: &Model,
        context: &Context,
        options: AzureOpenAIResponsesOptions,
    ) -> AssistantMessageEventStream {
        let stream = AssistantMessageEventStream::new();
        let model = model.clone();
        let context = context.clone();
        let s = stream.clone();
        tokio::spawn(async move {
            let mut output = AssistantMessage::empty(&model);
            if let Err(e) = run_azure(&model, &context, &options, &s, &mut output).await {
                finish_stream_error(&s, &mut output, e, false);
            }
        });
        stream
    }
}

async fn run_azure(
    model: &Model,
    context: &Context,
    options: &AzureOpenAIResponsesOptions,
    stream: &AssistantMessageEventStream,
    output: &mut AssistantMessage,
) -> Result<()> {
    let api_key = options
        .base
        .api_key
        .as_deref()
        .ok_or_else(|| anyhow!("No API key for provider: {}", model.provider))?;
    let deployment = resolve_deployment_name(model, options);
    let headers = merge_model_headers(model, Some(&options.base));
    let providers: HashSet<String> = AZURE_TOOL_CALL_PROVIDERS.iter().map(|s| s.to_string()).collect();
    let mut params = build_params(model, context, options, &deployment, &providers)?;
    params = apply_on_payload(options.base.on_payload.as_ref(), params, model).await;

    let (base_url, api_version) = resolve_azure_config(model, options)?;
    let client = build_http_client(options.base.timeout_ms)?;
    let url = format!("{base_url}/deployments/{deployment}/responses?api-version={api_version}");
    let mut req = client.post(&url).header("api-key", api_key).json(&params);
    for (k, v) in &headers {
        req = req.header(k, v);
    }
    let response = req.send().await?;
    invoke_on_response_from_reqwest(options.base.on_response.as_ref(), &response, model).await;
    let response = crate::api::common::check_response_ok(response).await?;

    stream.push(AssistantMessageEvent::Start {
        partial: output.clone(),
    });
    let events = collect_sse_json_events(response).await?;
    process_responses_stream(events, output, stream, model, None).await?;
    stream.push(AssistantMessageEvent::Done {
        reason: output.stop_reason,
        message: output.clone(),
    });
    stream.end();
    Ok(())
}

fn resolve_deployment_name(model: &Model, options: &AzureOpenAIResponsesOptions) -> String {
    if let Some(d) = &options.azure_deployment_name {
        return d.clone();
    }
    if let Some(map) = get_provider_env_value("AZURE_OPENAI_DEPLOYMENT_NAME_MAP", options.base.env.as_ref()) {
        for entry in map.split(',') {
            if let Some((id, dep)) = entry.split_once('=')
                && id.trim() == model.id
            {
                return dep.trim().to_string();
            }
        }
    }
    model.id.clone()
}

fn resolve_azure_config(model: &Model, options: &AzureOpenAIResponsesOptions) -> Result<(String, String)> {
    let api_version = options
        .azure_api_version
        .clone()
        .or_else(|| get_provider_env_value("AZURE_OPENAI_API_VERSION", options.base.env.as_ref()))
        .unwrap_or_else(|| DEFAULT_AZURE_API_VERSION.to_string());
    let base_url = options
        .azure_base_url
        .clone()
        .or_else(|| get_provider_env_value("AZURE_OPENAI_BASE_URL", options.base.env.as_ref()))
        .or_else(|| {
            options
                .azure_resource_name
                .clone()
                .or_else(|| get_provider_env_value("AZURE_OPENAI_RESOURCE_NAME", options.base.env.as_ref()))
                .map(|r| format!("https://{r}.openai.azure.com/openai/v1"))
        })
        .or_else(|| Some(model.base_url.clone()))
        .ok_or_else(|| anyhow!("Azure OpenAI base URL is required"))?;
    Ok((normalize_azure_base_url(&base_url), api_version))
}

fn normalize_azure_base_url(base_url: &str) -> String {
    let trimmed = base_url.trim().trim_end_matches('/');
    if let Ok(mut url) = url::Url::parse(trimmed) {
        let host = url.host_str().unwrap_or("").to_lowercase();
        let is_azure = host.ends_with(".openai.azure.com")
            || host.ends_with(".cognitiveservices.azure.com")
            || host.ends_with(".ai.azure.com");
        let path = url.path().trim_end_matches('/');
        if is_azure && (path.is_empty() || path == "/" || path == "/openai" || path == "/openai/v1/responses") {
            url.set_path("/openai/v1");
            url.set_query(None);
        }
        return url.to_string().trim_end_matches('/').to_string();
    }
    trimmed.to_string()
}

fn build_params(
    model: &Model,
    context: &Context,
    options: &AzureOpenAIResponsesOptions,
    deployment: &str,
    providers: &HashSet<String>,
) -> Result<Value> {
    let messages = convert_responses_messages(model, context, providers, None);
    let mut params = json!({
        "model": deployment,
        "input": messages,
        "stream": true,
        "prompt_cache_key": clamp_openai_prompt_cache_key(options.base.session_id.as_deref()),
        "store": false
    });
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
