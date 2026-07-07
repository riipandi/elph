use anyhow::{Result, anyhow};
use eventsource_stream::Eventsource;
use futures::StreamExt;
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use super::common::request_aborted_error;

/// Invoke `on_event` for each JSON event in an SSE response body.
pub async fn for_each_sse_json_event<F>(
    response: reqwest::Response,
    signal: &Option<CancellationToken>,
    mut on_event: F,
) -> Result<()>
where
    F: FnMut(Value) -> Result<()>,
{
    let mut stream = response.bytes_stream().eventsource();
    loop {
        let next_item = match signal {
            Some(token) => {
                let token = token.clone();
                tokio::select! {
                    item = stream.next() => item,
                    _ = token.cancelled() => return Err(request_aborted_error()),
                }
            }
            None => stream.next().await,
        };

        let Some(item) = next_item else {
            break;
        };

        let event = item?;
        let data = event.data.trim();
        if data.is_empty() || data == "[DONE]" {
            continue;
        }
        let value =
            serde_json::from_str::<Value>(data).map_err(|error| anyhow!("Invalid SSE JSON: {error}; data={data}"))?;
        on_event(value)?;
    }
    Ok(())
}

/// Collect all JSON events from an SSE response body.
pub async fn collect_sse_json_events(response: reqwest::Response) -> Result<Vec<Value>> {
    let mut events = Vec::new();
    for_each_sse_json_event(response, &None, |event| {
        events.push(event);
        Ok(())
    })
    .await?;
    Ok(events)
}

/// Anthropic-style SSE decoder state.
#[derive(Default)]
pub struct SseDecoderState {
    event: Option<String>,
    data: Vec<String>,
    raw: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ServerSentEvent {
    pub event: Option<String>,
    pub data: String,
    pub raw: Vec<String>,
}

pub(crate) fn flush_sse_event(state: &mut SseDecoderState) -> Option<ServerSentEvent> {
    if state.event.is_none() && state.data.is_empty() {
        return None;
    }
    let event = ServerSentEvent {
        event: state.event.take(),
        data: state.data.join("\n"),
        raw: std::mem::take(&mut state.raw),
    };
    state.data.clear();
    Some(event)
}

pub(crate) fn decode_sse_line(line: &str, state: &mut SseDecoderState) -> Option<ServerSentEvent> {
    if line.is_empty() {
        return flush_sse_event(state);
    }
    state.raw.push(line.to_string());
    if line.starts_with(':') {
        return None;
    }
    if let Some((field, value)) = line.split_once(':') {
        let value = value.strip_prefix(' ').unwrap_or(value);
        match field {
            "event" => state.event = Some(value.to_string()),
            "data" => state.data.push(value.to_string()),
            _ => {}
        }
    }
    None
}

/// Parse raw bytes into Anthropic SSE events.
pub fn decode_sse_buffer(buffer: &str, state: &mut SseDecoderState) -> Vec<ServerSentEvent> {
    let mut events = Vec::new();
    for line in buffer.split('\n') {
        let line = line.trim_end_matches('\r');
        if let Some(event) = decode_sse_line(line, state) {
            events.push(event);
        }
    }
    events
}

fn process_sse_line_buffer(line_buffer: &mut String, state: &mut SseDecoderState) -> Vec<ServerSentEvent> {
    let mut events = Vec::new();
    while let Some(newline_pos) = line_buffer.find('\n') {
        let line = line_buffer[..newline_pos].trim_end_matches('\r').to_string();
        *line_buffer = line_buffer[newline_pos + 1..].to_string();
        if let Some(event) = decode_sse_line(&line, state) {
            events.push(event);
        }
    }
    events
}

/// Invoke `on_event` for each Anthropic-style SSE event, reading the body incrementally.
pub async fn for_each_anthropic_sse_event<F>(
    response: reqwest::Response,
    signal: &Option<CancellationToken>,
    mut on_event: F,
) -> Result<()>
where
    F: FnMut(ServerSentEvent) -> Result<()>,
{
    let mut state = SseDecoderState::default();
    let mut line_buffer = String::new();
    let mut byte_stream = response.bytes_stream();

    loop {
        let next_chunk = match signal {
            Some(token) => {
                let token = token.clone();
                tokio::select! {
                    item = byte_stream.next() => item,
                    _ = token.cancelled() => return Err(request_aborted_error()),
                }
            }
            None => byte_stream.next().await,
        };

        let Some(chunk) = next_chunk else {
            break;
        };

        let chunk = chunk?;
        line_buffer.push_str(&String::from_utf8_lossy(&chunk));
        for event in process_sse_line_buffer(&mut line_buffer, &mut state) {
            on_event(event)?;
        }
    }

    if !line_buffer.is_empty()
        && let Some(event) = decode_sse_line(line_buffer.trim_end_matches('\r'), &mut state)
    {
        on_event(event)?;
    }
    if let Some(event) = flush_sse_event(&mut state) {
        on_event(event)?;
    }
    Ok(())
}

pub const ANTHROPIC_MESSAGE_EVENTS: &[&str] = &[
    "message_start",
    "message_delta",
    "message_stop",
    "content_block_start",
    "content_block_delta",
    "content_block_stop",
];
