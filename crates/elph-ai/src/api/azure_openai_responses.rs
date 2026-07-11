use std::collections::HashSet;

use anyhow::{Result, anyhow};

use serde_json::{Value, json};

use crate::api::azure_base_url::{build_default_azure_base_url, normalize_azure_base_url};
use crate::api::common::{
    apply_on_payload, build_http_client_for_target, finish_stream_error, invoke_on_response_from_reqwest,
    is_request_aborted, merge_model_headers, send_with_abort,
};
use crate::api::openai_prompt_cache::clamp_openai_prompt_cache_key;
use crate::api::openai_responses_shared::{
    ResponsesStreamState, convert_responses_messages, convert_responses_tools, process_responses_stream_event,
};
use crate::api::simple_options::build_base_options;
use crate::models::{clamp_thinking_level, thinking_level_to_str};
use crate::types::{
    AssistantMessage, AssistantMessageEvent, Context, Model, ProviderStreams, SimpleStreamOptions, StopReason,
    StreamOptions,
};
use crate::utils::event_stream::AssistantMessageEventStream;
use crate::utils::provider_env::get_provider_env_value;

use super::sse::for_each_sse_json_event;

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
                let aborted = crate::api::common::is_abort_error(&e);
                finish_stream_error(&s, &mut output, e, aborted);
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
    let url = format!("{base_url}/deployments/{deployment}/responses?api-version={api_version}");
    let client = build_http_client_for_target(options.base.timeout_ms, Some(&url), options.base.env.as_ref())?;
    let mut req = client.post(&url).header("api-key", api_key).json(&params);
    for (k, v) in &headers {
        req = req.header(k, v);
    }
    let response = send_with_abort(&options.base.signal, req).await?;
    invoke_on_response_from_reqwest(options.base.on_response.as_ref(), &response, model).await;
    let response = crate::api::common::check_response_ok(response).await?;

    stream.push(AssistantMessageEvent::Start {
        partial: output.clone(),
    });
    let mut responses_state = ResponsesStreamState::default();
    for_each_sse_json_event(response, &options.base.signal, |event| {
        process_responses_stream_event(&event, &mut responses_state, output, stream, model, None)
    })
    .await?;

    if is_request_aborted(&options.base.signal) {
        output.stop_reason = StopReason::Aborted;
    } else if !responses_state.saw_terminal {
        return Err(anyhow!("OpenAI Responses stream ended before a terminal response event"));
    }

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
                .map(|r| build_default_azure_base_url(&r))
        })
        .or_else(|| Some(model.base_url.clone()))
        .ok_or_else(|| anyhow!("Azure OpenAI base URL is required"))?;
    Ok((normalize_azure_base_url(&base_url)?, api_version))
}

fn build_params(
    model: &Model,
    context: &Context,
    options: &AzureOpenAIResponsesOptions,
    deployment: &str,
    providers: &HashSet<String>,
) -> Result<Value> {
    let supports_tool_search = model
        .openai_responses_compat
        .as_ref()
        .and_then(|c| c.supports_tool_search)
        .unwrap_or(false);
    let (immediate_tools, deferred_map) =
        crate::utils::deferred_tools::split_deferred_tools(context, supports_tool_search, None);
    let messages = convert_responses_messages(
        model,
        context,
        providers,
        Some(crate::api::openai_responses_shared::ConvertResponsesMessagesOptions {
            include_system_prompt: true,
            deferred_tools: Some(deferred_map),
        }),
    );
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
    if !immediate_tools.is_empty() {
        params["tools"] = json!(convert_responses_tools(&immediate_tools, None));
    }
    if model.reasoning
        && let Some(effort) = &options.reasoning_effort
    {
        params["reasoning"] = json!({ "effort": effort, "summary": options.reasoning_summary.clone().unwrap_or_else(|| "auto".to_string()) });
        params["include"] = json!(["reasoning.encrypted_content"]);
    }
    Ok(params)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::openai_prompt_cache::OPENAI_PROMPT_CACHE_KEY_MAX_LENGTH;
    use crate::types::{Message, UserContent};

    fn sample_context() -> Context {
        Context {
            system_prompt: None,
            messages: vec![Message::User {
                content: UserContent::Text("hello".to_string()),
                timestamp: 0,
            }],
            tools: None,
        }
    }

    fn sample_model() -> Model {
        get_builtin_model_for_test()
    }

    fn get_builtin_model_for_test() -> Model {
        crate::get_builtin_model("azure-openai-responses", "gpt-4o-mini").expect("model")
    }

    #[test]
    fn build_params_clamps_prompt_cache_key_and_disables_store() {
        let model = sample_model();
        let context = sample_context();
        let long_session = "x".repeat(OPENAI_PROMPT_CACHE_KEY_MAX_LENGTH + 3);
        let options = AzureOpenAIResponsesOptions {
            base: StreamOptions {
                session_id: Some(long_session),
                ..Default::default()
            },
            ..Default::default()
        };
        let providers: HashSet<String> = AZURE_TOOL_CALL_PROVIDERS.iter().map(|s| s.to_string()).collect();
        let params = build_params(&model, &context, &options, "gpt-4o-mini", &providers).expect("params");
        assert_eq!(
            params["prompt_cache_key"].as_str(),
            Some("x".repeat(OPENAI_PROMPT_CACHE_KEY_MAX_LENGTH).as_str())
        );
        assert_eq!(params["store"], false);
    }
}
