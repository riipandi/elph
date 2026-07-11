use anyhow::{Result, anyhow};

use serde_json::{Value, json};

use crate::api::common::{
    apply_on_payload, build_http_client_for_target, finish_stream_error, invoke_on_response_from_reqwest,
    is_request_aborted, merge_model_headers,
};
use crate::api::google_shared::{
    convert_messages, convert_tools, is_thinking_part, map_stop_reason_finish, map_tool_choice,
    retain_thought_signature,
};
use crate::api::simple_options::build_base_options;
use crate::models::{calculate_cost, clamp_thinking_level};
use crate::types::{
    AssistantContentBlock, AssistantMessage, AssistantMessageEvent, Context, Model, ProviderStreams,
    SimpleStreamOptions, StopReason, StreamOptions,
};
use crate::utils::event_stream::AssistantMessageEventStream;
use crate::utils::sanitize_unicode::sanitize_surrogates;

use super::sse::for_each_sse_json_event;

#[derive(Clone, Default)]
pub struct GoogleOptions {
    pub base: StreamOptions,
    pub tool_choice: Option<String>,
    pub thinking: Option<GoogleThinkingConfig>,
}

#[derive(Debug, Clone)]
pub struct GoogleThinkingConfig {
    pub enabled: bool,
    pub budget_tokens: Option<i32>,
    pub level: Option<String>,
}

pub struct GoogleGenerativeAIApi;
static TOOL_CALL_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

impl ProviderStreams for GoogleGenerativeAIApi {
    fn stream(&self, model: &Model, context: &Context, options: Option<StreamOptions>) -> AssistantMessageEventStream {
        self.stream_with_options(
            model,
            context,
            GoogleOptions {
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
        if opts.and_then(|o| o.reasoning).is_none() {
            return self.stream_with_options(
                model,
                context,
                GoogleOptions {
                    base,
                    thinking: Some(GoogleThinkingConfig {
                        enabled: false,
                        budget_tokens: None,
                        level: None,
                    }),
                    ..Default::default()
                },
            );
        }
        let reasoning = clamp_thinking_level(model, opts.unwrap().reasoning.unwrap());
        self.stream_with_options(
            model,
            context,
            GoogleOptions {
                base,
                thinking: Some(GoogleThinkingConfig {
                    enabled: true,
                    budget_tokens: Some(get_google_budget(model, reasoning)),
                    level: None,
                }),
                ..Default::default()
            },
        )
    }
}

impl GoogleGenerativeAIApi {
    pub fn stream_with_options(
        &self,
        model: &Model,
        context: &Context,
        options: GoogleOptions,
    ) -> AssistantMessageEventStream {
        let stream = AssistantMessageEventStream::new();
        let model = model.clone();
        let context = context.clone();
        let s = stream.clone();
        tokio::spawn(async move {
            let mut output = AssistantMessage::empty(&model);
            if let Err(e) = run_google(&model, &context, &options, &s, &mut output).await {
                let aborted = crate::api::common::is_abort_error(&e);
                finish_stream_error(&s, &mut output, e, aborted);
            }
        });
        stream
    }
}

async fn run_google(
    model: &Model,
    context: &Context,
    options: &GoogleOptions,
    stream: &AssistantMessageEventStream,
    output: &mut AssistantMessage,
) -> Result<()> {
    let api_key = options
        .base
        .api_key
        .as_deref()
        .ok_or_else(|| anyhow!("No API key for provider: {}", model.provider))?;
    let mut params = build_params(model, context, options)?;
    params = apply_on_payload(options.base.on_payload.as_ref(), params, model).await;
    let headers = merge_model_headers(model, Some(&options.base));

    let url = format!(
        "{}/v1beta/models/{}:streamGenerateContent?alt=sse&key={}",
        model.base_url.trim_end_matches('/'),
        model.id,
        api_key
    );
    let client = build_http_client_for_target(options.base.timeout_ms, Some(&url), options.base.env.as_ref())?;
    let mut req = client.post(&url).json(&params);
    for (k, v) in &headers {
        req = req.header(k, v);
    }
    let response = crate::api::common::send_with_abort(&options.base.signal, req).await?;
    invoke_on_response_from_reqwest(options.base.on_response.as_ref(), &response, model).await;
    let response = crate::api::common::check_response_ok(response).await?;

    stream.push(AssistantMessageEvent::Start {
        partial: output.clone(),
    });
    let mut current_block: Option<usize> = None;
    for_each_sse_json_event(response, &options.base.signal, |chunk| {
        output.response_id = output
            .response_id
            .clone()
            .or_else(|| chunk.get("responseId").and_then(|v| v.as_str()).map(|s| s.to_string()));
        if let Some(candidate) = chunk.get("candidates").and_then(|c| c.get(0)) {
            if let Some(parts) = candidate.pointer("/content/parts").and_then(|v| v.as_array()) {
                for part in parts {
                    if let Some(text) = part.get("text").and_then(|v| v.as_str()) {
                        let is_thinking = is_thinking_part(part);
                        let idx = ensure_block(output, stream, &mut current_block, is_thinking);
                        match &mut output.content[idx] {
                            AssistantContentBlock::Thinking(t) => {
                                t.thinking.push_str(text);
                                t.thinking_signature = retain_thought_signature(
                                    t.thinking_signature.as_deref(),
                                    part.get("thoughtSignature").and_then(|v| v.as_str()),
                                );
                                stream.push(AssistantMessageEvent::ThinkingDelta {
                                    content_index: idx,
                                    delta: text.to_string(),
                                    partial: output.clone(),
                                });
                            }
                            AssistantContentBlock::Text(t) => {
                                t.text.push_str(text);
                                t.text_signature = retain_thought_signature(
                                    t.text_signature.as_deref(),
                                    part.get("thoughtSignature").and_then(|v| v.as_str()),
                                );
                                stream.push(AssistantMessageEvent::TextDelta {
                                    content_index: idx,
                                    delta: text.to_string(),
                                    partial: output.clone(),
                                });
                            }
                            _ => {}
                        }
                    }
                    if let Some(fc) = part.get("functionCall") {
                        end_current_block(output, stream, &mut current_block);
                        let name = fc.get("name").and_then(|v| v.as_str()).unwrap_or("");
                        let id = fc
                            .get("id")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string())
                            .unwrap_or_else(|| {
                                format!(
                                    "{}_{}_{}",
                                    name,
                                    chrono::Utc::now().timestamp_millis(),
                                    TOOL_CALL_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                                )
                            });
                        let tc = crate::types::ToolCall::new(id, name, fc.get("args").cloned().unwrap_or(json!({})));
                        let idx = output.content.len();
                        output.content.push(AssistantContentBlock::ToolCall(tc.clone()));
                        stream.push(AssistantMessageEvent::ToolcallStart {
                            content_index: idx,
                            partial: output.clone(),
                        });
                        stream.push(AssistantMessageEvent::ToolcallDelta {
                            content_index: idx,
                            delta: tc.arguments.to_string(),
                            partial: output.clone(),
                        });
                        stream.push(AssistantMessageEvent::ToolcallEnd {
                            content_index: idx,
                            tool_call: tc,
                            partial: output.clone(),
                        });
                    }
                }
            }
            if let Some(reason) = candidate.get("finishReason").and_then(|v| v.as_str()) {
                output.stop_reason = map_stop_reason_finish(reason);
                if output.content.iter().any(|b| b.is_tool_call()) {
                    output.stop_reason = StopReason::ToolUse;
                }
            }
        }
        if let Some(meta) = chunk.get("usageMetadata") {
            let prompt = meta.get("promptTokenCount").and_then(|v| v.as_u64()).unwrap_or(0);
            let cached = meta
                .get("cachedContentTokenCount")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            output.usage.input = prompt.saturating_sub(cached);
            output.usage.output = meta.get("candidatesTokenCount").and_then(|v| v.as_u64()).unwrap_or(0)
                + meta.get("thoughtsTokenCount").and_then(|v| v.as_u64()).unwrap_or(0);
            output.usage.cache_read = cached;
            output.usage.reasoning = meta.get("thoughtsTokenCount").and_then(|v| v.as_u64());
            output.usage.total_tokens = meta.get("totalTokenCount").and_then(|v| v.as_u64()).unwrap_or(0);
            calculate_cost(model, &mut output.usage);
        }
        Ok(())
    })
    .await?;
    end_current_block(output, stream, &mut current_block);
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

fn ensure_block(
    output: &mut AssistantMessage,
    stream: &AssistantMessageEventStream,
    current: &mut Option<usize>,
    thinking: bool,
) -> usize {
    let need_new = current.is_none()
        || !matches!(
            (thinking, current.and_then(|i| output.content.get(i))),
            (true, Some(AssistantContentBlock::Thinking(_))) | (false, Some(AssistantContentBlock::Text(_)))
        );
    if need_new {
        end_current_block(output, stream, current);
        let idx = output.content.len();
        if thinking {
            output
                .content
                .push(AssistantContentBlock::Thinking(crate::types::ThinkingContent::new("")));
            stream.push(AssistantMessageEvent::ThinkingStart {
                content_index: idx,
                partial: output.clone(),
            });
        } else {
            output
                .content
                .push(AssistantContentBlock::Text(crate::types::TextContent::new("")));
            stream.push(AssistantMessageEvent::TextStart {
                content_index: idx,
                partial: output.clone(),
            });
        }
        *current = Some(idx);
    }
    current.unwrap()
}

fn end_current_block(output: &mut AssistantMessage, stream: &AssistantMessageEventStream, current: &mut Option<usize>) {
    if let Some(idx) = current.take() {
        match &output.content[idx] {
            AssistantContentBlock::Text(t) => stream.push(AssistantMessageEvent::TextEnd {
                content_index: idx,
                content: t.text.clone(),
                partial: output.clone(),
            }),
            AssistantContentBlock::Thinking(t) => stream.push(AssistantMessageEvent::ThinkingEnd {
                content_index: idx,
                content: t.thinking.clone(),
                partial: output.clone(),
            }),
            _ => {}
        }
    }
}

fn build_params(model: &Model, context: &Context, options: &GoogleOptions) -> Result<Value> {
    let contents = convert_messages(model, context);
    let mut generation_config = json!({});
    if let Some(temp) = options.base.temperature {
        generation_config["temperature"] = json!(temp);
    }
    if let Some(max) = options.base.max_tokens {
        generation_config["maxOutputTokens"] = json!(max);
    }
    let mut body = json!({ "contents": contents });
    if let Some(sp) = &context.system_prompt {
        body["systemInstruction"] = json!({ "parts": [{ "text": sanitize_surrogates(sp) }] });
    }
    if let Some(tools) = &context.tools {
        if let Some(t) = convert_tools(tools, false) {
            body["tools"] = json!(t);
        }
        if let Some(choice) = &options.tool_choice {
            body["toolConfig"] = json!({ "functionCallingConfig": { "mode": map_tool_choice(choice) } });
        }
    }
    if let Some(thinking) = &options.thinking
        && thinking.enabled
        && model.reasoning
    {
        let mut tc = json!({ "includeThoughts": true });
        if let Some(level) = &thinking.level {
            tc["thinkingLevel"] = json!(level);
        } else if let Some(budget) = thinking.budget_tokens {
            tc["thinkingBudget"] = json!(budget);
        }
        generation_config["thinkingConfig"] = tc;
    }
    if generation_config.as_object().map(|o| !o.is_empty()).unwrap_or(false) {
        body["generationConfig"] = generation_config;
    }
    Ok(body)
}

pub fn get_google_budget(model: &Model, effort: crate::types::ThinkingLevel) -> i32 {
    let level = match effort {
        crate::types::ThinkingLevel::Minimal => "minimal",
        crate::types::ThinkingLevel::Low => "low",
        crate::types::ThinkingLevel::Medium => "medium",
        crate::types::ThinkingLevel::High | crate::types::ThinkingLevel::Xhigh | crate::types::ThinkingLevel::Max => {
            "high"
        }
    };
    if model.id.contains("2.5-pro") {
        return match level {
            "minimal" => 128,
            "low" => 2048,
            "medium" => 8192,
            _ => 32768,
        };
    }
    if model.id.contains("2.5-flash-lite") {
        return match level {
            "minimal" => 512,
            "low" => 2048,
            "medium" => 8192,
            _ => 24576,
        };
    }
    if model.id.contains("2.5-flash") {
        return match level {
            "minimal" => 128,
            "low" => 2048,
            "medium" => 8192,
            _ => 24576,
        };
    }
    -1
}
