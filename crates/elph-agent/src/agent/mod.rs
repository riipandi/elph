//! Stateful `Agent` wrapper — elph-agent module.

mod queue;
mod state;

use std::collections::HashSet;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use elph_ai::{
    ImageContent, Message, OnPayloadCallback, OnResponseCallback, SimpleStreamOptions, ThinkingBudgets, Transport,
    UserContent, builtin_models,
};
use tokio::sync::{Mutex, oneshot};
use tokio_util::sync::CancellationToken;

use crate::agent_loop::{run_agent_loop, run_agent_loop_continue};
use crate::messages::default_convert_to_llm_fn;
use crate::types::{
    AgentContext, AgentEvent, AgentLoopConfig, AgentMessage, AgentState, AgentThinkingLevel, ConvertToLlmFn, QueueMode,
    StreamFn, ToolExecutionMode,
};

pub use queue::PendingMessageQueue;
pub use state::default_model;

pub type AgentListener =
    Arc<dyn Fn(AgentEvent, CancellationToken) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>;

/// Handle returned by [`Agent::subscribe`]. Call [`AgentSubscription::unsubscribe`] to remove the listener.
pub struct AgentSubscription {
    listeners: Arc<Mutex<Vec<AgentListener>>>,
    listener: AgentListener,
    active: Arc<std::sync::atomic::AtomicBool>,
}

impl AgentSubscription {
    pub async fn unsubscribe(self) {
        if self.active.swap(false, std::sync::atomic::Ordering::SeqCst) {
            let mut listeners = self.listeners.lock().await;
            listeners.retain(|entry| !Arc::ptr_eq(entry, &self.listener));
        }
    }
}

#[derive(Clone, Default)]
pub struct PartialAgentState {
    pub system_prompt: Option<String>,
    pub model: Option<elph_ai::Model>,
    pub thinking_level: Option<AgentThinkingLevel>,
    pub tools: Option<Vec<crate::types::AgentTool>>,
    pub messages: Option<Vec<AgentMessage>>,
}

#[derive(Clone, Default)]
pub struct AgentOptions {
    pub initial_state: Option<PartialAgentState>,
    pub convert_to_llm: Option<ConvertToLlmFn>,
    pub transform_context: Option<crate::types::TransformContextFn>,
    pub stream_fn: Option<StreamFn>,
    pub get_api_key: Option<crate::types::GetApiKeyFn>,
    pub on_payload: Option<OnPayloadCallback>,
    pub on_response: Option<OnResponseCallback>,
    pub before_tool_call: Option<crate::types::BeforeToolCallFn>,
    pub after_tool_call: Option<crate::types::AfterToolCallFn>,
    pub prepare_next_turn: Option<crate::types::PrepareNextTurnFn>,
    pub prepare_next_turn_legacy: Option<crate::types::PrepareNextTurnLegacyFn>,
    pub steering_mode: QueueMode,
    pub follow_up_mode: QueueMode,
    pub session_id: Option<String>,
    pub thinking_budgets: Option<ThinkingBudgets>,
    pub transport: Option<Transport>,
    pub max_retry_delay_ms: Option<u64>,
    pub tool_execution: ToolExecutionMode,
}

struct ActiveRun {
    idle_tx: oneshot::Sender<()>,
    idle_rx: Mutex<Option<oneshot::Receiver<()>>>,
    abort_token: CancellationToken,
}

pub struct Agent {
    state: Arc<Mutex<state::MutableAgentState>>,
    listeners: Arc<Mutex<Vec<AgentListener>>>,
    steering_queue: PendingMessageQueue,
    follow_up_queue: PendingMessageQueue,
    convert_to_llm: ConvertToLlmFn,
    transform_context: Option<crate::types::TransformContextFn>,
    stream_fn: StreamFn,
    get_api_key: Option<crate::types::GetApiKeyFn>,
    on_payload: Option<OnPayloadCallback>,
    on_response: Option<OnResponseCallback>,
    before_tool_call: Option<crate::types::BeforeToolCallFn>,
    after_tool_call: Option<crate::types::AfterToolCallFn>,
    prepare_next_turn: Option<crate::types::PrepareNextTurnFn>,
    prepare_next_turn_legacy: Option<crate::types::PrepareNextTurnLegacyFn>,
    current_abort_token: Arc<Mutex<Option<CancellationToken>>>,
    session_id: Arc<std::sync::Mutex<Option<String>>>,
    thinking_budgets: Option<ThinkingBudgets>,
    transport: Transport,
    max_retry_delay_ms: Option<u64>,
    tool_execution: ToolExecutionMode,
    active_run: Mutex<Option<ActiveRun>>,
    skip_initial_steering: Arc<std::sync::atomic::AtomicBool>,
}

impl Agent {
    pub fn new(options: AgentOptions) -> Self {
        let models = builtin_models(None);
        let default_stream: StreamFn = Arc::new(move |model, context, opts| models.stream_simple(model, context, opts));

        Self {
            state: Arc::new(Mutex::new(state::MutableAgentState::from_partial(
                options.initial_state,
            ))),
            listeners: Arc::new(Mutex::new(Vec::new())),
            steering_queue: PendingMessageQueue::new(options.steering_mode),
            follow_up_queue: PendingMessageQueue::new(options.follow_up_mode),
            convert_to_llm: options.convert_to_llm.unwrap_or_else(default_convert_to_llm_fn),
            transform_context: options.transform_context,
            stream_fn: options.stream_fn.unwrap_or(default_stream),
            get_api_key: options.get_api_key,
            on_payload: options.on_payload,
            on_response: options.on_response,
            before_tool_call: options.before_tool_call,
            after_tool_call: options.after_tool_call,
            prepare_next_turn: options.prepare_next_turn,
            prepare_next_turn_legacy: options.prepare_next_turn_legacy,
            current_abort_token: Arc::new(Mutex::new(None)),
            session_id: Arc::new(std::sync::Mutex::new(options.session_id)),
            thinking_budgets: options.thinking_budgets,
            transport: options.transport.unwrap_or(Transport::Auto),
            max_retry_delay_ms: options.max_retry_delay_ms,
            tool_execution: options.tool_execution,
            active_run: Mutex::new(None),
            skip_initial_steering: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    pub async fn subscribe(&self, listener: AgentListener) -> AgentSubscription {
        let listener = listener;
        self.listeners.lock().await.push(listener.clone());
        AgentSubscription {
            listeners: self.listeners.clone(),
            listener,
            active: Arc::new(std::sync::atomic::AtomicBool::new(true)),
        }
    }

    pub fn session_id(&self) -> Option<String> {
        self.session_id.lock().expect("session id lock").clone()
    }

    pub fn set_session_id(&self, session_id: Option<String>) {
        *self.session_id.lock().expect("session id lock") = session_id;
    }

    pub async fn signal(&self) -> Option<CancellationToken> {
        self.active_run.lock().await.as_ref().map(|run| run.abort_token.clone())
    }

    pub async fn set_system_prompt(&self, prompt: impl Into<String>) {
        self.state.lock().await.set_system_prompt(prompt.into());
    }

    pub async fn set_model(&self, model: elph_ai::Model) {
        self.state.lock().await.set_model(model);
    }

    pub async fn set_thinking_level(&self, level: AgentThinkingLevel) {
        self.state.lock().await.set_thinking_level(level);
    }

    pub async fn set_tools(&self, tools: Vec<crate::types::AgentTool>) {
        self.state.lock().await.set_tools(tools);
    }

    pub async fn set_messages(&self, messages: Vec<AgentMessage>) {
        self.state.lock().await.set_messages(messages);
    }

    pub async fn append_message(&self, message: AgentMessage) {
        self.state.lock().await.append_message(message);
    }

    pub async fn clear_messages(&self) {
        self.state.lock().await.clear_messages();
    }

    pub async fn state(&self) -> AgentState {
        self.state.lock().await.snapshot()
    }

    pub fn set_steering_mode(&self, mode: QueueMode) {
        self.steering_queue.set_mode(mode);
    }

    pub fn steering_mode(&self) -> QueueMode {
        self.steering_queue.mode()
    }

    pub fn set_follow_up_mode(&self, mode: QueueMode) {
        self.follow_up_queue.set_mode(mode);
    }

    pub fn follow_up_mode(&self) -> QueueMode {
        self.follow_up_queue.mode()
    }

    pub fn steer(&self, message: AgentMessage) {
        self.steering_queue.enqueue(message);
    }

    pub fn follow_up(&self, message: AgentMessage) {
        self.follow_up_queue.enqueue(message);
    }

    pub fn clear_steering_queue(&self) {
        self.steering_queue.clear();
    }

    pub fn clear_follow_up_queue(&self) {
        self.follow_up_queue.clear();
    }

    pub fn clear_all_queues(&self) {
        self.clear_steering_queue();
        self.clear_follow_up_queue();
    }

    pub fn has_queued_messages(&self) -> bool {
        self.steering_queue.has_items() || self.follow_up_queue.has_items()
    }

    pub async fn abort(&self) {
        if let Some(run) = self.active_run.lock().await.as_ref() {
            run.abort_token.cancel();
        }
    }

    pub async fn wait_for_idle(&self) {
        let rx = {
            let guard = self.active_run.lock().await;
            if let Some(run) = guard.as_ref() {
                run.idle_rx.lock().await.take()
            } else {
                None
            }
        };
        if let Some(rx) = rx {
            let _ = rx.await;
        }
    }

    pub async fn reset(&self) {
        self.state.lock().await.reset();
        self.clear_all_queues();
    }

    pub async fn prompt_text(&self, text: impl Into<String>, images: Option<Vec<ImageContent>>) -> anyhow::Result<()> {
        let mut content: Vec<elph_ai::ContentBlock> = vec![elph_ai::ContentBlock::Text { text: text.into() }];
        if let Some(images) = images {
            for image in images {
                content.push(elph_ai::ContentBlock::Image {
                    data: image.data,
                    mime_type: image.mime_type,
                });
            }
        }
        self.prompt_messages(vec![crate::types::llm_message_to_agent(Message::User {
            content: UserContent::Blocks(content),
            timestamp: now_ms(),
        })])
        .await
    }

    pub async fn prompt_messages(&self, messages: Vec<AgentMessage>) -> anyhow::Result<()> {
        if self.active_run.lock().await.is_some() {
            anyhow::bail!(
                "Agent is already processing a prompt. Use steer() or followUp() to queue messages, or wait for completion."
            );
        }
        self.run_prompt_messages(messages, false).await
    }

    pub async fn continue_run(&self) -> anyhow::Result<()> {
        if self.active_run.lock().await.is_some() {
            anyhow::bail!("Agent is already processing. Wait for completion before continuing.");
        }

        let last = self.state.lock().await.messages().last().cloned();
        let Some(last) = last else {
            anyhow::bail!("No messages to continue from");
        };

        if last.role() == "assistant" {
            let steering = self.steering_queue.drain();
            if !steering.is_empty() {
                return self.run_prompt_messages(steering, true).await;
            }
            let follow_up = self.follow_up_queue.drain();
            if !follow_up.is_empty() {
                return self.run_prompt_messages(follow_up, false).await;
            }
            anyhow::bail!("Cannot continue from message role: assistant");
        }

        self.run_continuation().await
    }

    async fn run_prompt_messages(
        &self,
        messages: Vec<AgentMessage>,
        skip_initial_steering: bool,
    ) -> anyhow::Result<()> {
        self.skip_initial_steering
            .store(skip_initial_steering, std::sync::atomic::Ordering::SeqCst);
        let context = self.create_context_snapshot().await;
        let config = self.create_loop_config();
        let token = self.begin_run().await?;
        let emit = self.create_emit_callback(token.clone());

        run_agent_loop(messages, context, config, emit, Some(token))
            .await
            .map_err(|error| anyhow::anyhow!(error))?;
        self.finish_run().await;
        Ok(())
    }

    async fn run_continuation(&self) -> anyhow::Result<()> {
        self.skip_initial_steering
            .store(false, std::sync::atomic::Ordering::SeqCst);
        let context = self.create_context_snapshot().await;
        let config = self.create_loop_config();
        let token = self.begin_run().await?;
        let emit = self.create_emit_callback(token.clone());

        run_agent_loop_continue(context, config, emit, Some(token))
            .await
            .map_err(|error| anyhow::anyhow!(error))?;
        self.finish_run().await;
        Ok(())
    }

    async fn create_context_snapshot(&self) -> AgentContext {
        let state = self.state.lock().await;
        AgentContext {
            system_prompt: state.system_prompt().to_string(),
            messages: state.messages().to_vec(),
            tools: state.tools().to_vec(),
        }
    }

    fn create_loop_config(&self) -> AgentLoopConfig {
        let steering_queue = self.steering_queue.clone();
        let follow_up_queue = self.follow_up_queue.clone();
        let skip_flag = self.skip_initial_steering.clone();

        let get_steering: crate::types::GetQueuedMessagesFn = Arc::new({
            let steering_queue = steering_queue.clone();
            let skip_flag = skip_flag.clone();
            move || {
                let steering_queue = steering_queue.clone();
                let skip_flag = skip_flag.clone();
                Box::pin(async move {
                    if skip_flag.swap(false, std::sync::atomic::Ordering::SeqCst) {
                        return Vec::new();
                    }
                    steering_queue.drain()
                })
            }
        });

        let get_follow_up: crate::types::GetQueuedMessagesFn = Arc::new({
            let follow_up_queue = follow_up_queue.clone();
            move || {
                let follow_up_queue = follow_up_queue.clone();
                Box::pin(async move { follow_up_queue.drain() })
            }
        });

        let state = self.state.try_lock().ok();
        let (model, thinking_level) = state
            .as_ref()
            .map(|s| (s.model().clone(), s.thinking_level()))
            .unwrap_or_else(|| (default_model(), AgentThinkingLevel::Off));

        let mut stream_options = SimpleStreamOptions {
            base: Default::default(),
            reasoning: thinking_level.to_stream_reasoning(),
            thinking_budgets: self.thinking_budgets.clone(),
        };
        stream_options.base.session_id = self.session_id.lock().expect("session id lock").clone();
        stream_options.base.transport = Some(self.transport);
        stream_options.base.max_retry_delay_ms = self.max_retry_delay_ms;
        stream_options.base.on_payload = self.on_payload.clone();
        stream_options.base.on_response = self.on_response.clone();

        AgentLoopConfig {
            model,
            stream_options,
            convert_to_llm: self.convert_to_llm.clone(),
            transform_context: self.transform_context.clone(),
            get_api_key: self.get_api_key.clone(),
            should_stop_after_turn: None,
            prepare_next_turn: {
                let with_context = self.prepare_next_turn.clone();
                let legacy = self.prepare_next_turn_legacy.clone();
                let token_holder = self.current_abort_token.clone();
                if with_context.is_none() && legacy.is_none() {
                    None
                } else {
                    Some(Arc::new(move |ctx| {
                        let with_context = with_context.clone();
                        let legacy = legacy.clone();
                        let token_holder = token_holder.clone();
                        Box::pin(async move {
                            if let Some(callback) = with_context {
                                callback(ctx).await
                            } else if let Some(callback) = legacy {
                                let signal = token_holder.lock().await.clone();
                                callback(signal).await
                            } else {
                                None
                            }
                        })
                    }))
                }
            },
            get_steering_messages: Some(get_steering),
            get_follow_up_messages: Some(get_follow_up),
            tool_execution: self.tool_execution,
            before_tool_call: self.before_tool_call.clone(),
            after_tool_call: self.after_tool_call.clone(),
            stream_fn: Some(self.stream_fn.clone()),
        }
    }

    fn create_emit_callback(&self, token: CancellationToken) -> crate::agent_loop::AgentEventCallback {
        let state = self.state.clone();
        let listeners = self.listeners.clone();

        Arc::new(move |event| {
            let state = state.clone();
            let listeners = listeners.clone();
            let token = token.clone();
            Box::pin(async move {
                process_event(&state, &event, &listeners, &token).await;
            })
        })
    }

    async fn begin_run(&self) -> anyhow::Result<CancellationToken> {
        if self.active_run.lock().await.is_some() {
            anyhow::bail!("Agent is already processing.");
        }
        let (idle_tx, idle_rx) = oneshot::channel();
        let abort_token = CancellationToken::new();
        {
            let mut state = self.state.lock().await;
            state.set_streaming(true);
            state.clear_error();
        }
        *self.current_abort_token.lock().await = Some(abort_token.clone());
        *self.active_run.lock().await = Some(ActiveRun {
            idle_tx,
            idle_rx: Mutex::new(Some(idle_rx)),
            abort_token: abort_token.clone(),
        });
        Ok(abort_token)
    }

    async fn finish_run(&self) {
        {
            let mut state = self.state.lock().await;
            state.set_streaming(false);
            state.set_streaming_message(None);
            state.set_pending_tool_calls(HashSet::new());
        }
        *self.current_abort_token.lock().await = None;
        if let Some(run) = self.active_run.lock().await.take() {
            let _ = run.idle_tx.send(());
        }
    }
}

async fn process_event(
    state: &Arc<Mutex<state::MutableAgentState>>,
    event: &AgentEvent,
    listeners: &Arc<Mutex<Vec<AgentListener>>>,
    token: &CancellationToken,
) {
    {
        let mut state = state.lock().await;
        match event {
            AgentEvent::MessageStart { message } => {
                state.set_streaming_message(Some(message.clone()));
            }
            AgentEvent::MessageUpdate { message, .. } => {
                state.set_streaming_message(Some(message.clone()));
            }
            AgentEvent::MessageEnd { message } => {
                state.set_streaming_message(None);
                state.push_message(message.clone());
            }
            AgentEvent::ToolExecutionStart { tool_call_id, .. } => {
                state.add_pending_tool_call(tool_call_id.clone());
            }
            AgentEvent::ToolExecutionEnd { tool_call_id, .. } => {
                state.remove_pending_tool_call(tool_call_id);
            }
            AgentEvent::TurnEnd { message, .. } => {
                if let AgentMessage::Llm(message) = message
                    && let Message::Assistant(assistant) = message.as_ref()
                    && assistant.error_message.is_some()
                {
                    state.set_error_message(assistant.error_message.clone());
                }
            }
            AgentEvent::AgentEnd { .. } => {
                state.set_streaming_message(None);
            }
            _ => {}
        }
    }

    let listeners = listeners.lock().await;
    for listener in listeners.iter() {
        listener(event.clone(), token.clone()).await;
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
