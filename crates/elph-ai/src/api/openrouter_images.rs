use std::future::Future;
use std::pin::Pin;

use anyhow::{Result, anyhow};
use serde_json::{Value, json};

use crate::api::common::{apply_on_payload, build_http_client, invoke_on_response_from_reqwest, merge_model_headers};
use crate::types::{
    AssistantImages, ContentBlock, ImagesContext, ImagesModel, ImagesOptions, ProviderImages, StopReason,
};
use crate::utils::error_body::{format_provider_error, normalize_provider_error};
use crate::utils::sanitize_unicode::sanitize_surrogates;

pub struct OpenRouterImagesApi;

impl ProviderImages for OpenRouterImagesApi {
    fn generate_images(
        &self,
        model: &ImagesModel,
        context: &ImagesContext,
        options: Option<ImagesOptions>,
    ) -> Pin<Box<dyn Future<Output = AssistantImages> + Send>> {
        let model = model.clone();
        let context = context.clone();
        let options = options.unwrap_or(ImagesOptions {
            api_key: None,
            signal: None,
            env: None,
            headers: None,
            timeout_ms: None,
            max_retries: None,
            on_payload: None,
            on_response: None,
        });
        Box::pin(async move { generate_images_inner(&model, &context, &options).await })
    }
}

async fn generate_images_inner(
    model: &ImagesModel,
    context: &ImagesContext,
    options: &ImagesOptions,
) -> AssistantImages {
    let mut output = AssistantImages {
        api: model.api.clone(),
        provider: model.provider.clone(),
        model: model.id.clone(),
        output: vec![],
        response_id: None,
        usage: None,
        stop_reason: StopReason::Stop,
        error_message: None,
        timestamp: chrono::Utc::now().timestamp_millis(),
    };

    match run_generate(model, context, options).await {
        Ok(result) => result,
        Err(error) => {
            if error.to_string() == "Request aborted" {
                output.stop_reason = StopReason::Aborted;
                output.error_message = Some("Request aborted".to_string());
            } else {
                output.stop_reason = StopReason::Error;
                output.error_message = Some(format_provider_error(&normalize_provider_error(&error), None));
            }
            output
        }
    }
}

async fn run_generate(
    model: &ImagesModel,
    context: &ImagesContext,
    options: &ImagesOptions,
) -> Result<AssistantImages> {
    let api_key = options
        .api_key
        .as_deref()
        .ok_or_else(|| anyhow!("No API key for provider: {}", model.provider))?;
    let mut params = build_params(model, context);
    params = apply_on_payload(
        options.on_payload.as_ref(),
        params,
        &crate::types::Model {
            id: model.id.clone(),
            name: model.name.clone(),
            api: model.api.clone(),
            provider: model.provider.clone(),
            base_url: model.base_url.clone(),
            reasoning: false,
            thinking_level_map: None,
            input: model.input.clone(),
            cost: model.cost,
            context_window: 0,
            max_tokens: 0,
            headers: model.headers.clone(),
            openai_completions_compat: None,
            openai_responses_compat: None,
            anthropic_compat: None,
        },
    )
    .await;

    let headers = merge_model_headers(
        &crate::types::Model {
            id: model.id.clone(),
            name: model.name.clone(),
            api: model.api.clone(),
            provider: model.provider.clone(),
            base_url: model.base_url.clone(),
            reasoning: false,
            thinking_level_map: None,
            input: model.input.clone(),
            cost: model.cost,
            context_window: 0,
            max_tokens: 0,
            headers: model.headers.clone(),
            openai_completions_compat: None,
            openai_responses_compat: None,
            anthropic_compat: None,
        },
        None,
    );

    if options.signal.as_ref().is_some_and(|token| token.is_cancelled()) {
        return Err(anyhow!("Request aborted"));
    }

    let client = build_http_client(options.timeout_ms)?;
    let url = format!("{}/chat/completions", model.base_url.trim_end_matches('/'));
    let mut req = client.post(&url).bearer_auth(api_key).json(&params);
    for (k, v) in &headers {
        req = req.header(k, v);
    }
    let response = match &options.signal {
        Some(token) => {
            let token = token.clone();
            tokio::select! {
                result = req.send() => result?,
                _ = token.cancelled() => return Err(anyhow!("Request aborted")),
            }
        }
        None => req.send().await?,
    };
    invoke_on_response_from_reqwest(
        options.on_response.as_ref(),
        &response,
        &crate::types::Model {
            id: model.id.clone(),
            name: model.name.clone(),
            api: model.api.clone(),
            provider: model.provider.clone(),
            base_url: model.base_url.clone(),
            reasoning: false,
            thinking_level_map: None,
            input: model.input.clone(),
            cost: model.cost,
            context_window: 0,
            max_tokens: 0,
            headers: model.headers.clone(),
            openai_completions_compat: None,
            openai_responses_compat: None,
            anthropic_compat: None,
        },
    )
    .await;
    let response = crate::api::common::check_response_ok(response).await?;
    let body: Value = response.json().await?;

    let mut output = AssistantImages {
        api: model.api.clone(),
        provider: model.provider.clone(),
        model: model.id.clone(),
        output: vec![],
        response_id: body.get("id").and_then(|v| v.as_str()).map(|s| s.to_string()),
        usage: body.get("usage").map(|u| parse_usage(u, model)),
        stop_reason: StopReason::Stop,
        error_message: None,
        timestamp: chrono::Utc::now().timestamp_millis(),
    };

    if let Some(choice) = body.get("choices").and_then(|c| c.get(0)) {
        if let Some(content) = choice.pointer("/message/content").and_then(|v| v.as_str())
            && !content.is_empty()
        {
            output.output.push(ContentBlock::Text {
                text: content.to_string(),
            });
        }
        if let Some(images) = choice.pointer("/message/images").and_then(|v| v.as_array()) {
            let data_url_re = regex::Regex::new(r"^data:([^;]+);base64,(.+)$").ok();
            for image in images {
                let image_url = image
                    .get("image_url")
                    .and_then(|v| v.as_str().or_else(|| v.get("url").and_then(|u| u.as_str())));
                if let Some(url) = image_url
                    && let Some(caps) = data_url_re.as_ref().and_then(|re| re.captures(url))
                {
                    output.output.push(ContentBlock::Image {
                        mime_type: caps.get(1).unwrap().as_str().to_string(),
                        data: caps.get(2).unwrap().as_str().to_string(),
                    });
                }
            }
        }
    }
    Ok(output)
}

fn build_params(model: &ImagesModel, context: &ImagesContext) -> Value {
    let content: Vec<Value> = context
        .input
        .iter()
        .map(|item| match item {
            ContentBlock::Text { text } => json!({ "type": "text", "text": sanitize_surrogates(text) }),
            ContentBlock::Image { data, mime_type } => json!({
                "type": "image_url",
                "image_url": { "url": format!("data:{mime_type};base64,{data}") }
            }),
        })
        .collect();
    let modalities: Vec<&str> = if model.output.iter().any(|o| o == "text") {
        vec!["image", "text"]
    } else {
        vec!["image"]
    };
    json!({
        "model": model.id,
        "messages": [{ "role": "user", "content": content }],
        "stream": false,
        "modalities": modalities
    })
}

fn parse_usage(raw: &Value, model: &ImagesModel) -> crate::types::Usage {
    let prompt = raw.get("prompt_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
    let reported_cached = raw
        .pointer("/prompt_tokens_details/cached_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let cache_write = raw
        .pointer("/prompt_tokens_details/cache_write_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let cache_read = if cache_write > 0 {
        reported_cached.saturating_sub(cache_write)
    } else {
        reported_cached
    };
    let input = prompt.saturating_sub(cache_read).saturating_sub(cache_write);
    let output = raw.get("completion_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
    let m = 1_000_000.0;
    let cost = crate::types::UsageCost {
        input: (model.cost.input / m) * input as f64,
        output: (model.cost.output / m) * output as f64,
        cache_read: (model.cost.cache_read / m) * cache_read as f64,
        cache_write: (model.cost.cache_write / m) * cache_write as f64,
        total: 0.0,
    };
    let mut usage = crate::types::Usage {
        input,
        output,
        cache_read,
        cache_write,
        cache_write_1h: None,
        reasoning: None,
        total_tokens: input + output + cache_read + cache_write,
        cost,
    };
    usage.cost.total = usage.cost.input + usage.cost.output + usage.cost.cache_read + usage.cost.cache_write;
    usage
}
