//! Amazon Bedrock Converse Stream API with bearer-token or SigV4 (AWS SDK) auth.

use anyhow::{Result, anyhow};
use aws_config::BehaviorVersion;
use aws_sdk_bedrockruntime::Client as BedrockClient;
use aws_sdk_bedrockruntime::types::{
    ContentBlock as BedrockContentBlock, ConversationRole, ConverseStreamOutput, Message as BedrockMessage,
    SystemContentBlock,
};

use serde_json::{Value, json};

use crate::api::common::{
    apply_on_payload, build_http_client, finish_stream_error, invoke_on_response_from_reqwest, merge_model_headers,
};
use crate::api::simple_options::{adjust_max_tokens_for_thinking, build_base_options, clamp_max_tokens_to_context};
use crate::api::transform_messages::transform_messages;
use crate::models::calculate_cost;
use crate::types::{
    AssistantContentBlock, AssistantMessage, AssistantMessageEvent, ContentBlock, Context, Message, Model,
    ProviderStreams, SimpleStreamOptions, StopReason, StreamOptions, UserContent,
};
use crate::utils::event_stream::AssistantMessageEventStream;
use crate::utils::json_parse::parse_streaming_json;
use crate::utils::provider_env::get_provider_env_value;
use crate::utils::sanitize_unicode::sanitize_surrogates;

use super::sse::collect_sse_json_events;

#[derive(Clone, Default)]
pub struct BedrockOptions {
    pub base: StreamOptions,
    pub region: Option<String>,
    pub bearer_token: Option<String>,
    pub tool_choice: Option<String>,
    pub reasoning: Option<crate::types::ThinkingLevel>,
}

pub struct BedrockConverseStreamApi;

impl ProviderStreams for BedrockConverseStreamApi {
    fn stream(&self, model: &Model, context: &Context, options: Option<StreamOptions>) -> AssistantMessageEventStream {
        self.stream_with_options(
            model,
            context,
            BedrockOptions {
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
        let base = build_base_options(model, context, opts, None);
        if opts.and_then(|o| o.reasoning).is_none() {
            return self.stream_with_options(
                model,
                context,
                BedrockOptions {
                    base,
                    ..Default::default()
                },
            );
        }
        let reasoning = opts.unwrap().reasoning;
        let (max_tokens, thinking_budget) = adjust_max_tokens_for_thinking(
            base.max_tokens,
            model.max_tokens,
            reasoning.unwrap(),
            opts.and_then(|o| o.thinking_budgets.as_ref()),
        );
        let max_tokens = clamp_max_tokens_to_context(model, context, max_tokens);
        self.stream_with_options(
            model,
            context,
            BedrockOptions {
                base: StreamOptions {
                    max_tokens: Some(max_tokens),
                    ..base
                },
                reasoning,
                ..Default::default()
            },
        )
    }
}

impl BedrockConverseStreamApi {
    pub fn stream_with_options(
        &self,
        model: &Model,
        context: &Context,
        options: BedrockOptions,
    ) -> AssistantMessageEventStream {
        let stream = AssistantMessageEventStream::new();
        let model = model.clone();
        let context = context.clone();
        let s = stream.clone();
        tokio::spawn(async move {
            let mut output = AssistantMessage::empty(&model);
            if let Err(e) = run_bedrock(&model, &context, &options, &s, &mut output).await {
                finish_stream_error(&s, &mut output, e, false);
            }
        });
        stream
    }
}

async fn run_bedrock(
    model: &Model,
    context: &Context,
    options: &BedrockOptions,
    stream: &AssistantMessageEventStream,
    output: &mut AssistantMessage,
) -> Result<()> {
    let bearer = options
        .bearer_token
        .clone()
        .or_else(|| get_provider_env_value("AWS_BEARER_TOKEN_BEDROCK", options.base.env.as_ref()));

    let region = options
        .region
        .clone()
        .or_else(|| get_provider_env_value("AWS_REGION", options.base.env.as_ref()))
        .or_else(|| get_provider_env_value("AWS_DEFAULT_REGION", options.base.env.as_ref()))
        .unwrap_or_else(|| "us-east-1".to_string());

    let mut body = build_converse_body(model, context, options)?;
    body = apply_on_payload(options.base.on_payload.as_ref(), body, model).await;
    let headers = merge_model_headers(model, Some(&options.base));

    let events = if let Some(bearer) = bearer {
        run_bedrock_bearer(&region, model, &body, &headers, &bearer, options).await?
    } else {
        run_bedrock_sigv4(&region, model, &body, options).await?
    };

    let mut blocks: Vec<(usize, String)> = Vec::new();
    for event in events {
        if event.get("messageStart").is_some() {
            stream.push(AssistantMessageEvent::Start {
                partial: output.clone(),
            });
        } else if let Some(start) = event.get("contentBlockStart") {
            let idx = start.get("contentBlockIndex").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            if let Some(tool) = start.pointer("/start/toolUse") {
                let block_idx = output.content.len();
                output
                    .content
                    .push(AssistantContentBlock::ToolCall(crate::types::ToolCall::new(
                        tool.get("toolUseId").and_then(|v| v.as_str()).unwrap_or(""),
                        tool.get("name").and_then(|v| v.as_str()).unwrap_or(""),
                        json!({}),
                    )));
                blocks.push((idx, String::new()));
                stream.push(AssistantMessageEvent::ToolcallStart {
                    content_index: block_idx,
                    partial: output.clone(),
                });
            }
        } else if let Some(delta) = event.get("contentBlockDelta") {
            let idx = delta.get("contentBlockIndex").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            if let Some(text) = delta.pointer("/delta/text").and_then(|v| v.as_str()) {
                let block_idx = output.content.len();
                if !blocks.iter().any(|(i, _)| *i == idx) {
                    output
                        .content
                        .push(AssistantContentBlock::Text(crate::types::TextContent::new("")));
                    blocks.push((idx, String::new()));
                    stream.push(AssistantMessageEvent::TextStart {
                        content_index: block_idx,
                        partial: output.clone(),
                    });
                }
                let pos = blocks.iter().position(|(i, _)| *i == idx).unwrap_or(block_idx);
                if let AssistantContentBlock::Text(t) = &mut output.content[pos] {
                    t.text.push_str(text);
                    stream.push(AssistantMessageEvent::TextDelta {
                        content_index: pos,
                        delta: text.to_string(),
                        partial: output.clone(),
                    });
                }
            }
            if let Some(input) = delta.pointer("/delta/toolUse/input").and_then(|v| v.as_str()) {
                if let Some(pos) = blocks.iter().position(|(i, _)| *i == idx) {
                    blocks[pos].1.push_str(input);
                    if let AssistantContentBlock::ToolCall(tc) = &mut output.content[pos] {
                        tc.arguments = parse_streaming_json(Some(&blocks[pos].1));
                        stream.push(AssistantMessageEvent::ToolcallDelta {
                            content_index: pos,
                            delta: input.to_string(),
                            partial: output.clone(),
                        });
                    }
                }
            }
            if let Some(text) = delta
                .pointer("/delta/reasoningContent/reasoningText/text")
                .and_then(|v| v.as_str())
            {
                let block_idx = output.content.len();
                output
                    .content
                    .push(AssistantContentBlock::Thinking(crate::types::ThinkingContent::new(
                        text,
                    )));
                stream.push(AssistantMessageEvent::ThinkingStart {
                    content_index: block_idx,
                    partial: output.clone(),
                });
                stream.push(AssistantMessageEvent::ThinkingDelta {
                    content_index: block_idx,
                    delta: text.to_string(),
                    partial: output.clone(),
                });
            }
        } else if let Some(stop) = event.get("messageStop") {
            output.stop_reason = map_stop_reason(stop.get("stopReason").and_then(|v| v.as_str()));
        } else if let Some(meta) = event.get("metadata") {
            if let Some(usage) = meta.get("usage") {
                output.usage.input = usage.get("inputTokens").and_then(|v| v.as_u64()).unwrap_or(0);
                output.usage.output = usage.get("outputTokens").and_then(|v| v.as_u64()).unwrap_or(0);
                output.usage.cache_read = usage.get("cacheReadInputTokens").and_then(|v| v.as_u64()).unwrap_or(0);
                output.usage.cache_write = usage.get("cacheWriteInputTokens").and_then(|v| v.as_u64()).unwrap_or(0);
                output.usage.total_tokens = usage.get("totalTokens").and_then(|v| v.as_u64()).unwrap_or(0);
                calculate_cost(model, &mut output.usage);
            }
        }
    }
    stream.push(AssistantMessageEvent::Done {
        reason: output.stop_reason,
        message: output.clone(),
    });
    stream.end();
    Ok(())
}

/// Build Bedrock Converse request body (used by integration tests mirroring pi-ai).
pub fn build_bedrock_converse_body(model: &Model, context: &Context, options: &BedrockOptions) -> Result<Value> {
    build_converse_body(model, context, options)
}

fn build_converse_body(model: &Model, context: &Context, options: &BedrockOptions) -> Result<Value> {
    let messages = convert_messages(context, model);
    let mut body = json!({
        "messages": messages,
        "inferenceConfig": {}
    });
    if let Some(sp) = &context.system_prompt {
        body["system"] = json!([{ "text": sanitize_surrogates(sp) }]);
    }
    if let Some(max) = options.base.max_tokens {
        body["inferenceConfig"]["maxTokens"] = json!(max);
    }
    if let Some(temp) = options.base.temperature {
        body["inferenceConfig"]["temperature"] = json!(temp);
    }
    if let Some(tools) = &context.tools {
        if !tools.is_empty() {
            body["toolConfig"] = json!({
                "tools": tools.iter().map(|t| json!({
                    "toolSpec": {
                        "name": t.name,
                        "description": t.description,
                        "inputSchema": { "json": t.parameters }
                    }
                })).collect::<Vec<_>>()
            });
        }
    }
    Ok(body)
}

const EMPTY_TEXT_PLACEHOLDER: &str = "<empty>";

fn create_non_blank_text_block(text: &str) -> Option<Value> {
    let sanitized = sanitize_surrogates(text);
    if sanitized.trim().is_empty() {
        None
    } else {
        Some(json!({ "text": sanitized }))
    }
}

fn create_required_text_block(text: &str) -> Value {
    create_non_blank_text_block(text).unwrap_or_else(|| json!({ "text": EMPTY_TEXT_PLACEHOLDER }))
}

fn convert_tool_result_content(content: &[ContentBlock]) -> Vec<Value> {
    let mut result = Vec::new();
    for block in content {
        match block {
            ContentBlock::Text { text } => {
                if let Some(text_block) = create_non_blank_text_block(text) {
                    result.push(text_block);
                }
            }
            ContentBlock::Image { data, mime_type } => {
                result.push(json!({
                    "image": {
                        "format": mime_type.strip_prefix("image/").unwrap_or("png"),
                        "source": { "bytes": data }
                    }
                }));
            }
        }
    }
    if result.is_empty() {
        result.push(json!({ "text": EMPTY_TEXT_PLACEHOLDER }));
    }
    result
}

fn convert_messages(context: &Context, model: &Model) -> Vec<Value> {
    let transformed = transform_messages(context.messages.clone(), model, |id, _, _| {
        let s: String = id
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        s.chars().take(64).collect()
    });
    let mut result = Vec::new();
    let mut index = 0usize;
    while index < transformed.len() {
        match &transformed[index] {
            Message::User { content, .. } => {
                let mut blocks = match content {
                    UserContent::Text(t) => vec![create_required_text_block(t)],
                    UserContent::Blocks(bs) => {
                        let mut blocks = Vec::new();
                        for block in bs {
                            match block {
                                ContentBlock::Text { text } => {
                                    if let Some(text_block) = create_non_blank_text_block(text) {
                                        blocks.push(text_block);
                                    }
                                }
                                ContentBlock::Image { data, mime_type } => {
                                    blocks.push(json!({
                                        "image": {
                                            "format": mime_type.strip_prefix("image/").unwrap_or("png"),
                                            "source": { "bytes": data }
                                        }
                                    }));
                                }
                            }
                        }
                        if blocks.is_empty() {
                            blocks.push(json!({ "text": EMPTY_TEXT_PLACEHOLDER }));
                        }
                        blocks
                    }
                };
                result.push(json!({ "role": "user", "content": blocks }));
                index += 1;
            }
            Message::Assistant(a) => {
                if a.content.is_empty() {
                    index += 1;
                    continue;
                }
                let blocks: Vec<Value> = a
                    .content
                    .iter()
                    .filter_map(|b| match b {
                        AssistantContentBlock::Text(t) => create_non_blank_text_block(&t.text),
                        AssistantContentBlock::ToolCall(tc) => Some(json!({
                            "toolUse": { "toolUseId": tc.id, "name": tc.name, "input": tc.arguments }
                        })),
                        AssistantContentBlock::Thinking(t) => {
                            let thinking = sanitize_surrogates(&t.thinking);
                            if thinking.trim().is_empty() {
                                None
                            } else if t.thinking_signature.as_deref().unwrap_or("").trim().is_empty() {
                                Some(json!({ "text": thinking }))
                            } else {
                                Some(json!({
                                    "reasoningContent": {
                                        "reasoningText": {
                                            "text": thinking,
                                            "signature": t.thinking_signature.clone().unwrap_or_default()
                                        }
                                    }
                                }))
                            }
                        }
                    })
                    .collect();
                if !blocks.is_empty() {
                    result.push(json!({ "role": "assistant", "content": blocks }));
                }
                index += 1;
            }
            Message::ToolResult {
                tool_call_id,
                content,
                is_error,
                ..
            } => {
                let mut tool_results = vec![json!({
                    "toolResult": {
                        "toolUseId": tool_call_id,
                        "content": convert_tool_result_content(content),
                        "status": if *is_error { "error" } else { "success" }
                    }
                })];
                let mut j = index + 1;
                while j < transformed.len() {
                    if let Message::ToolResult {
                        tool_call_id,
                        content,
                        is_error,
                        ..
                    } = &transformed[j]
                    {
                        tool_results.push(json!({
                            "toolResult": {
                                "toolUseId": tool_call_id,
                                "content": convert_tool_result_content(content),
                                "status": if *is_error { "error" } else { "success" }
                            }
                        }));
                        j += 1;
                    } else {
                        break;
                    }
                }
                result.push(json!({ "role": "user", "content": tool_results }));
                index = j;
            }
        }
    }
    result
}

async fn run_bedrock_bearer(
    region: &str,
    model: &Model,
    body: &Value,
    headers: &std::collections::HashMap<String, String>,
    bearer: &str,
    options: &BedrockOptions,
) -> Result<Vec<Value>> {
    let client = build_http_client(options.base.timeout_ms)?;
    let url = format!(
        "https://bedrock-runtime.{region}.amazonaws.com/model/{}/converse-stream",
        model.id
    );
    let mut req = client
        .post(&url)
        .header("Authorization", format!("Bearer {bearer}"))
        .json(body);
    for (k, v) in headers {
        req = req.header(k, v);
    }
    let response = req.send().await?;
    invoke_on_response_from_reqwest(options.base.on_response.as_ref(), &response, model).await;
    let response = crate::api::common::check_response_ok(response).await?;
    collect_sse_json_events(response).await
}

async fn run_bedrock_sigv4(region: &str, model: &Model, body: &Value, _options: &BedrockOptions) -> Result<Vec<Value>> {
    let mut loader = aws_config::defaults(BehaviorVersion::latest());
    if let Ok(profile) = std::env::var("AWS_PROFILE") {
        loader = loader.profile_name(profile);
    }
    let config = loader.region(aws_config::Region::new(region.to_string())).load().await;
    let client = BedrockClient::new(&config);

    let mut builder = client.converse_stream().model_id(&model.id);
    if let Some(messages) = body.get("messages").and_then(|v| v.as_array()) {
        let sdk_messages: Vec<BedrockMessage> = messages.iter().filter_map(json_to_bedrock_message).collect();
        if !sdk_messages.is_empty() {
            builder = builder.set_messages(Some(sdk_messages));
        }
    }
    if let Some(system) = body.get("system").and_then(|v| v.as_array()) {
        let blocks: Vec<SystemContentBlock> = system
            .iter()
            .filter_map(|item| {
                let text = item.get("text")?.as_str()?;
                Some(SystemContentBlock::Text(text.to_string()))
            })
            .collect();
        if !blocks.is_empty() {
            builder = builder.set_system(Some(blocks));
        }
    }
    if let Some(inference) = body.get("inferenceConfig") {
        if let Some(max) = inference.get("maxTokens").and_then(|v| v.as_i64()) {
            builder = builder.inference_config(
                aws_sdk_bedrockruntime::types::InferenceConfiguration::builder()
                    .max_tokens(max as i32)
                    .build(),
            );
        }
    }

    let mut response = builder
        .send()
        .await
        .map_err(|e| anyhow!("Bedrock SigV4 request failed: {e}"))?;
    let mut events = Vec::new();
    loop {
        match response.stream.recv().await {
            Ok(Some(item)) => {
                if let Some(json) = sdk_stream_event_to_json(item) {
                    events.push(json);
                }
            }
            Ok(None) => break,
            Err(e) => return Err(anyhow!("Bedrock stream error: {e}")),
        }
    }
    Ok(events)
}

fn json_to_bedrock_message(value: &Value) -> Option<BedrockMessage> {
    let role = match value.get("role")?.as_str()? {
        "user" => ConversationRole::User,
        "assistant" => ConversationRole::Assistant,
        _ => return None,
    };
    let content: Vec<BedrockContentBlock> = value
        .get("content")?
        .as_array()?
        .iter()
        .filter_map(|block| {
            if let Some(text) = block.get("text").and_then(|v| v.as_str()) {
                return Some(BedrockContentBlock::Text(text.to_string()));
            }
            if let Some(tool_use) = block.get("toolUse") {
                let id = tool_use.get("toolUseId")?.as_str()?;
                let name = tool_use.get("name")?.as_str()?;
                let input = tool_use.get("input").cloned().unwrap_or(json!({}));
                return Some(BedrockContentBlock::ToolUse(
                    aws_sdk_bedrockruntime::types::ToolUseBlock::builder()
                        .tool_use_id(id)
                        .name(name)
                        .input(aws_smithy_types::Document::Object(
                            input
                                .as_object()
                                .map(|m| m.iter().map(|(k, v)| (k.clone(), json_to_document(v))).collect())
                                .unwrap_or_default(),
                        ))
                        .build()
                        .ok()?,
                ));
            }
            if let Some(tool_result) = block.get("toolResult") {
                let id = tool_result.get("toolUseId")?.as_str()?;
                let text = tool_result
                    .pointer("/content/0/text")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                return Some(BedrockContentBlock::ToolResult(
                    aws_sdk_bedrockruntime::types::ToolResultBlock::builder()
                        .tool_use_id(id)
                        .content(aws_sdk_bedrockruntime::types::ToolResultContentBlock::Text(
                            text.to_string(),
                        ))
                        .build()
                        .ok()?,
                ));
            }
            None
        })
        .collect();
    BedrockMessage::builder()
        .role(role)
        .set_content(Some(content))
        .build()
        .ok()
}

fn json_to_document(value: &Value) -> aws_smithy_types::Document {
    match value {
        Value::Null => aws_smithy_types::Document::Null,
        Value::Bool(b) => aws_smithy_types::Document::Bool(*b),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                aws_smithy_types::Document::Number(aws_smithy_types::Number::PosInt(i as u64))
            } else if let Some(f) = n.as_f64() {
                aws_smithy_types::Document::Number(aws_smithy_types::Number::Float(f))
            } else {
                aws_smithy_types::Document::String(n.to_string())
            }
        }
        Value::String(s) => aws_smithy_types::Document::String(s.clone()),
        Value::Array(arr) => aws_smithy_types::Document::Array(arr.iter().map(json_to_document).collect()),
        Value::Object(obj) => {
            aws_smithy_types::Document::Object(obj.iter().map(|(k, v)| (k.clone(), json_to_document(v))).collect())
        }
    }
}

fn sdk_stream_event_to_json(event: ConverseStreamOutput) -> Option<Value> {
    match event {
        ConverseStreamOutput::MessageStart(v) => Some(json!({ "messageStart": { "role": format!("{:?}", v.role()) } })),
        ConverseStreamOutput::ContentBlockStart(v) => Some(json!({
            "contentBlockStart": {
                "contentBlockIndex": v.content_block_index(),
                "start": v.start().map(|s| match s {
                    aws_sdk_bedrockruntime::types::ContentBlockStart::ToolUse(t) => json!({
                        "toolUse": { "toolUseId": t.tool_use_id(), "name": t.name() }
                    }),
                    _ => Value::Null,
                })
            }
        })),
        ConverseStreamOutput::ContentBlockDelta(v) => {
            let delta = match v.delta() {
                Some(aws_sdk_bedrockruntime::types::ContentBlockDelta::Text(t)) => json!({ "text": t }),
                Some(aws_sdk_bedrockruntime::types::ContentBlockDelta::ToolUse(t)) => {
                    json!({ "toolUse": { "input": t.input() } })
                }
                Some(aws_sdk_bedrockruntime::types::ContentBlockDelta::ReasoningContent(r)) => {
                    let text = r.as_text().cloned().unwrap_or_default();
                    json!({ "reasoningContent": { "reasoningText": { "text": text } } })
                }
                _ => Value::Null,
            };
            Some(json!({ "contentBlockDelta": { "contentBlockIndex": v.content_block_index(), "delta": delta } }))
        }
        ConverseStreamOutput::MessageStop(v) => Some(json!({
            "messageStop": { "stopReason": stop_reason_to_str(v.stop_reason().clone()) }
        })),
        ConverseStreamOutput::Metadata(v) => {
            let usage = v.usage();
            Some(json!({
                "metadata": {
                    "usage": {
                        "inputTokens": usage.map(|u| u.input_tokens()).unwrap_or(0),
                        "outputTokens": usage.map(|u| u.output_tokens()).unwrap_or(0),
                        "cacheReadInputTokens": usage.and_then(|u| u.cache_read_input_tokens()).unwrap_or(0),
                        "cacheWriteInputTokens": usage.and_then(|u| u.cache_write_input_tokens()).unwrap_or(0),
                        "totalTokens": usage.map(|u| u.total_tokens()).unwrap_or(0),
                    }
                }
            }))
        }
        _ => None,
    }
}

fn stop_reason_to_str(reason: aws_sdk_bedrockruntime::types::StopReason) -> String {
    format!("{reason:?}").replace("StopReason::", "").to_lowercase()
}

fn map_stop_reason(reason: Option<&str>) -> StopReason {
    match reason {
        Some("end_turn") | Some("stop_sequence") => StopReason::Stop,
        Some("max_tokens") | Some("model_context_window_exceeded") => StopReason::Length,
        Some("tool_use") => StopReason::ToolUse,
        _ => StopReason::Error,
    }
}
