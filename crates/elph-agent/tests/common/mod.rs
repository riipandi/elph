//! Shared test helpers for elph-agent integration tests.

#![allow(dead_code)]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use elph_ai::api::faux::RegisterFauxProviderOptions;
use elph_ai::{FauxProviderHandle, Model, Models, SimpleStreamOptions, builtin_models, create_models, faux_provider};

pub fn faux_stream_fn(faux: &FauxProviderHandle) -> elph_agent::StreamFn {
    let provider = faux.provider.clone();
    Arc::new(move |model, context, options| provider.stream_simple(model, context, options))
}

fn next_faux_provider_id() -> String {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let id = COUNTER.fetch_add(1, Ordering::SeqCst);
    format!("faux-{id}")
}

/// Unique faux provider per call so concurrent tests do not share response queues.
pub fn new_faux_with_options(options: RegisterFauxProviderOptions) -> FauxProviderHandle {
    faux_provider(RegisterFauxProviderOptions {
        provider: Some(next_faux_provider_id()),
        ..options
    })
}

/// Unique faux provider per call so concurrent harness tests do not share response queues.
pub fn new_faux() -> (FauxProviderHandle, Arc<Models>) {
    let faux = new_faux_with_options(Default::default());
    let mut models = create_models(None);
    models.set_provider(faux.provider.clone());
    (faux, models.into_arc())
}

pub fn faux_models(faux: &FauxProviderHandle) -> Arc<Models> {
    let mut models = builtin_models(None);
    models.set_provider(faux.provider.clone());
    models.into_arc()
}

pub fn base_loop_config(model: Model, stream_fn: elph_agent::StreamFn) -> elph_agent::AgentLoopConfig {
    elph_agent::AgentLoopConfig {
        model,
        stream_options: SimpleStreamOptions {
            base: Default::default(),
            reasoning: None,
            thinking_budgets: None,
        },
        convert_to_llm: elph_agent::default_convert_to_llm_fn(),
        transform_context: None,
        get_api_key: None,
        should_stop_after_turn: None,
        prepare_next_turn: None,
        get_steering_messages: None,
        get_follow_up_messages: None,
        tool_execution: elph_agent::ToolExecutionMode::Parallel,
        before_tool_call: None,
        after_tool_call: None,
        stream_fn: Some(stream_fn),
    }
}

pub fn empty_context() -> elph_agent::AgentContext {
    elph_agent::AgentContext {
        system_prompt: String::new(),
        messages: Vec::new(),
        tools: Vec::new(),
    }
}

pub fn test_context(system_prompt: &str) -> elph_agent::AgentContext {
    elph_agent::AgentContext {
        system_prompt: system_prompt.into(),
        messages: Vec::new(),
        tools: Vec::new(),
    }
}

pub fn capture_stream_options() -> (
    elph_agent::StreamFn,
    Arc<std::sync::Mutex<Vec<Option<elph_ai::StreamOptions>>>>,
) {
    let captured = Arc::new(std::sync::Mutex::new(Vec::new()));
    let captured_clone = captured.clone();
    let stream_fn: elph_agent::StreamFn = Arc::new(move |model, context, options| {
        captured_clone
            .lock()
            .expect("capture lock")
            .push(options.as_ref().map(|o| o.base.clone()));
        let models = builtin_models(None);
        models.stream_simple(model, context, options)
    });
    (stream_fn, captured)
}

pub fn user_texts(messages: &[elph_ai::Message]) -> Vec<String> {
    use elph_ai::{ContentBlock, Message, UserContent};
    messages
        .iter()
        .filter(|message| message.role() == "user")
        .filter_map(|message| match message {
            Message::User { content, .. } => match content {
                UserContent::Text(text) => Some(vec![text.clone()]),
                UserContent::Blocks(blocks) => Some(
                    blocks
                        .iter()
                        .filter_map(|block| match block {
                            ContentBlock::Text { text } => Some(text.clone()),
                            _ => None,
                        })
                        .collect(),
                ),
            },
            _ => None,
        })
        .flatten()
        .collect()
}

pub fn hanging_until_abort_stream_fn(model: &Model) -> elph_agent::StreamFn {
    let model = model.clone();
    Arc::new(move |_model, _context, options| {
        let stream = elph_ai::utils::event_stream::AssistantMessageEventStream::new();
        let signal = options.and_then(|o| o.base.signal.clone());
        let s = stream.clone();
        let model = model.clone();
        tokio::spawn(async move {
            while !signal.as_ref().is_some_and(|token| token.is_cancelled()) {
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
            let mut message = elph_ai::AssistantMessage::empty(&model);
            message.stop_reason = elph_ai::StopReason::Aborted;
            message.error_message = Some("Aborted".into());
            s.push(elph_ai::AssistantMessageEvent::Error {
                reason: elph_ai::StopReason::Aborted,
                error: message,
            });
            s.end();
        });
        stream
    })
}

pub fn capture_events() -> (
    elph_agent::agent_loop::AgentEventCallback,
    Arc<tokio::sync::Mutex<Vec<elph_agent::AgentEvent>>>,
) {
    let events = Arc::new(tokio::sync::Mutex::new(Vec::new()));
    let events_capture = events.clone();
    let emit: elph_agent::agent_loop::AgentEventCallback = Arc::new(move |event| {
        let events_capture = events_capture.clone();
        Box::pin(async move {
            events_capture.lock().await.push(event);
        })
    });
    (emit, events)
}

pub fn user_agent_message(text: &str) -> elph_agent::AgentMessage {
    elph_agent::llm_message_to_agent(elph_ai::Message::User {
        content: elph_ai::UserContent::Text(text.into()),
        timestamp: 0,
    })
}

pub fn assistant_agent_message(text: &str) -> elph_agent::AgentMessage {
    elph_agent::AgentMessage::Llm(Box::new(elph_ai::Message::Assistant(elph_ai::faux_assistant_message(
        vec![elph_ai::faux_text(text)],
        None,
    ))))
}

pub fn message_entry(
    id: &str,
    parent_id: Option<&str>,
    message: elph_agent::AgentMessage,
) -> elph_agent::SessionTreeEntry {
    elph_agent::SessionTreeEntry::Message {
        id: id.to_string(),
        parent_id: parent_id.map(str::to_string),
        timestamp: "2026-01-01T00:00:00.000Z".to_string(),
        message,
    }
}

pub fn label_entry(
    id: &str,
    parent_id: &str,
    target_id: &str,
    label: Option<&str>,
    timestamp: &str,
) -> elph_agent::SessionTreeEntry {
    elph_agent::SessionTreeEntry::Label {
        id: id.to_string(),
        parent_id: Some(parent_id.to_string()),
        timestamp: timestamp.to_string(),
        target_id: target_id.to_string(),
        label: label.map(str::to_string),
    }
}

pub fn error_stream_fn(message: &'static str) -> elph_agent::StreamFn {
    Arc::new(move |model, _context, _options| {
        let stream = elph_ai::utils::event_stream::AssistantMessageEventStream::new();
        let model = model.clone();
        let s = stream.clone();
        tokio::spawn(async move {
            let mut assistant = elph_ai::AssistantMessage::empty(&model);
            assistant.stop_reason = elph_ai::StopReason::Error;
            assistant.error_message = Some(message.into());
            s.push(elph_ai::AssistantMessageEvent::Error {
                reason: elph_ai::StopReason::Error,
                error: assistant,
            });
            s.end();
        });
        stream
    })
}
