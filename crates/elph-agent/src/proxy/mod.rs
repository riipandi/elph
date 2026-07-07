//! Browser stream proxy — ported from pi-agent `proxy.ts`.

use elph_ai::utils::event_stream::AssistantMessageEventStream;
use elph_ai::utils::json_parse::parse_streaming_json;
use elph_ai::{
    AssistantContentBlock, AssistantMessage, AssistantMessageEvent, Context, Model, SimpleStreamOptions, StopReason,
    ToolCall,
};
use futures::StreamExt;
use reqwest::Client;
use serde::Deserialize;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

/// Proxy stream options — server manages auth and forwards provider requests.
#[derive(Clone)]
pub struct ProxyStreamOptions {
    pub base: SimpleStreamOptions,
    pub auth_token: String,
    pub proxy_url: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ProxyAssistantMessageEvent {
    Start,
    TextStart {
        content_index: usize,
    },
    TextDelta {
        content_index: usize,
        delta: String,
    },
    TextEnd {
        content_index: usize,
        content_signature: Option<String>,
    },
    ThinkingStart {
        content_index: usize,
    },
    ThinkingDelta {
        content_index: usize,
        delta: String,
    },
    ThinkingEnd {
        content_index: usize,
        content_signature: Option<String>,
    },
    ToolcallStart {
        content_index: usize,
        id: String,
        tool_name: String,
    },
    ToolcallDelta {
        content_index: usize,
        delta: String,
    },
    ToolcallEnd {
        content_index: usize,
    },
    Done {
        reason: StopReason,
        usage: elph_ai::Usage,
    },
    Error {
        reason: StopReason,
        error_message: Option<String>,
        usage: elph_ai::Usage,
    },
}

/// Stream through a proxy server instead of calling providers directly.
pub fn stream_proxy(model: &Model, context: &Context, options: ProxyStreamOptions) -> AssistantMessageEventStream {
    let stream = AssistantMessageEventStream::new();
    let stream_clone = stream.clone();
    let model = model.clone();
    let context = context.clone();
    let model_for_error = model.clone();

    tokio::spawn(async move {
        if let Err(error) = run_proxy_stream(stream_clone.clone(), model, context, options).await {
            let mut partial = AssistantMessage::empty(&model_for_error);
            partial.stop_reason = StopReason::Error;
            partial.error_message = Some(error.to_string());
            stream_clone.push(AssistantMessageEvent::Error {
                reason: StopReason::Error,
                error: partial,
            });
        }
        stream_clone.end();
    });

    stream
}

async fn run_proxy_stream(
    stream: AssistantMessageEventStream,
    model: Model,
    context: Context,
    options: ProxyStreamOptions,
) -> anyhow::Result<()> {
    let client = Client::new();
    let url = format!("{}/api/stream", options.proxy_url.trim_end_matches('/'));
    let body = build_proxy_request_body(&model, &context, &options);

    let response = client
        .post(url)
        .header("Authorization", format!("Bearer {}", options.auth_token))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let error_body = response.text().await.unwrap_or_default();
        let message = serde_json::from_str::<Value>(&error_body)
            .ok()
            .and_then(|value| value.get("error").and_then(|v| v.as_str()).map(str::to_string))
            .unwrap_or_else(|| format!("Proxy error: {status}"));
        anyhow::bail!(message);
    }

    let mut partial = AssistantMessage::empty(&model);
    let mut byte_stream = response.bytes_stream();
    let mut buffer = String::new();

    while let Some(chunk) = byte_stream.next().await {
        if options
            .base
            .base
            .signal
            .as_ref()
            .is_some_and(CancellationToken::is_cancelled)
        {
            anyhow::bail!("Request aborted by user");
        }
        let chunk = chunk?;
        buffer.push_str(&String::from_utf8_lossy(&chunk));
        while let Some(line_end) = buffer.find('\n') {
            let line = buffer.drain(..=line_end).collect::<String>();
            let line = line.trim();
            if let Some(data) = line.strip_prefix("data: ") {
                let data = data.trim();
                if data.is_empty() {
                    continue;
                }
                let proxy_event: ProxyAssistantMessageEvent = serde_json::from_str(data)?;
                if let Some(event) = process_proxy_event(proxy_event, &mut partial) {
                    stream.push(event);
                }
            }
        }
    }

    Ok(())
}

fn build_proxy_request_body(model: &Model, context: &Context, options: &ProxyStreamOptions) -> Value {
    json!({
        "model": {
            "id": model.id,
            "name": model.name,
            "api": model.api,
            "provider": model.provider,
            "baseUrl": model.base_url,
        },
        "context": {
            "systemPrompt": context.system_prompt,
            "messages": context.messages.len(),
            "tools": context.tools.as_ref().map(|tools| tools.len()),
        },
        "options": {
            "temperature": options.base.base.temperature,
            "maxTokens": options.base.base.max_tokens,
            "reasoning": options.base.reasoning,
            "sessionId": options.base.base.session_id,
            "transport": options.base.base.transport,
            "maxRetryDelayMs": options.base.base.max_retry_delay_ms,
        }
    })
}

fn process_proxy_event(
    proxy_event: ProxyAssistantMessageEvent,
    partial: &mut AssistantMessage,
) -> Option<AssistantMessageEvent> {
    match proxy_event {
        ProxyAssistantMessageEvent::Start => Some(AssistantMessageEvent::Start {
            partial: partial.clone(),
        }),
        ProxyAssistantMessageEvent::TextStart { content_index } => {
            ensure_content_slot(partial, content_index);
            partial.content[content_index] = AssistantContentBlock::Text(elph_ai::TextContent::new(""));
            Some(AssistantMessageEvent::TextStart {
                content_index,
                partial: partial.clone(),
            })
        }
        ProxyAssistantMessageEvent::TextDelta { content_index, delta } => {
            if let Some(AssistantContentBlock::Text(text)) = partial.content.get_mut(content_index) {
                text.text.push_str(&delta);
                return Some(AssistantMessageEvent::TextDelta {
                    content_index,
                    delta,
                    partial: partial.clone(),
                });
            }
            None
        }
        ProxyAssistantMessageEvent::TextEnd {
            content_index,
            content_signature,
        } => {
            if let Some(AssistantContentBlock::Text(text)) = partial.content.get_mut(content_index) {
                text.text_signature = content_signature;
            }
            let content = match partial.content.get(content_index) {
                Some(AssistantContentBlock::Text(text)) => text.text.clone(),
                _ => String::new(),
            };
            Some(AssistantMessageEvent::TextEnd {
                content_index,
                content,
                partial: partial.clone(),
            })
        }
        ProxyAssistantMessageEvent::ThinkingStart { content_index } => {
            ensure_content_slot(partial, content_index);
            partial.content[content_index] = AssistantContentBlock::Thinking(elph_ai::ThinkingContent::new(""));
            Some(AssistantMessageEvent::ThinkingStart {
                content_index,
                partial: partial.clone(),
            })
        }
        ProxyAssistantMessageEvent::ThinkingDelta { content_index, delta } => {
            if let Some(AssistantContentBlock::Thinking(text)) = partial.content.get_mut(content_index) {
                text.thinking.push_str(&delta);
                return Some(AssistantMessageEvent::ThinkingDelta {
                    content_index,
                    delta,
                    partial: partial.clone(),
                });
            }
            None
        }
        ProxyAssistantMessageEvent::ThinkingEnd {
            content_index,
            content_signature,
        } => {
            if let Some(AssistantContentBlock::Thinking(text)) = partial.content.get_mut(content_index) {
                text.thinking_signature = content_signature;
            }
            let content = match partial.content.get(content_index) {
                Some(AssistantContentBlock::Thinking(text)) => text.thinking.clone(),
                _ => String::new(),
            };
            Some(AssistantMessageEvent::ThinkingEnd {
                content_index,
                content,
                partial: partial.clone(),
            })
        }
        ProxyAssistantMessageEvent::ToolcallStart {
            content_index,
            id,
            tool_name,
        } => {
            ensure_content_slot(partial, content_index);
            partial.content[content_index] = AssistantContentBlock::ToolCall(ToolCall::new(id, tool_name, Value::Null));
            Some(AssistantMessageEvent::ToolcallStart {
                content_index,
                partial: partial.clone(),
            })
        }
        ProxyAssistantMessageEvent::ToolcallDelta { content_index, delta } => {
            if let Some(AssistantContentBlock::ToolCall(tool)) = partial.content.get_mut(content_index) {
                let mut partial_json = serde_json::to_string(&tool.arguments).unwrap_or_default();
                if partial_json == "null" {
                    partial_json.clear();
                }
                partial_json.push_str(&delta);
                tool.arguments = parse_streaming_json(Some(&partial_json));
                return Some(AssistantMessageEvent::ToolcallDelta {
                    content_index,
                    delta,
                    partial: partial.clone(),
                });
            }
            None
        }
        ProxyAssistantMessageEvent::ToolcallEnd { content_index } => {
            let tool_call = match partial.content.get(content_index) {
                Some(AssistantContentBlock::ToolCall(tool)) => tool.clone(),
                _ => return None,
            };
            Some(AssistantMessageEvent::ToolcallEnd {
                content_index,
                tool_call,
                partial: partial.clone(),
            })
        }
        ProxyAssistantMessageEvent::Done { reason, usage } => {
            partial.stop_reason = reason;
            partial.usage = usage;
            Some(AssistantMessageEvent::Done {
                reason,
                message: partial.clone(),
            })
        }
        ProxyAssistantMessageEvent::Error {
            reason,
            error_message,
            usage,
        } => {
            partial.stop_reason = reason;
            partial.error_message = error_message;
            partial.usage = usage;
            Some(AssistantMessageEvent::Error {
                reason,
                error: partial.clone(),
            })
        }
    }
}

fn ensure_content_slot(message: &mut AssistantMessage, index: usize) {
    if message.content.len() <= index {
        message
            .content
            .resize_with(index + 1, || AssistantContentBlock::Text(elph_ai::TextContent::new("")));
    }
}
