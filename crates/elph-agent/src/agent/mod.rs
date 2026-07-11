//! Stateful `Agent` wrapper — elph-agent module.

mod events;
mod queue;
mod run;
mod state;

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use elph_ai::{OnPayloadCallback, OnResponseCallback, ThinkingBudgets, Transport, builtin_models};
use parking_lot::Mutex as ParkingMutex;
use tokio::sync::{Mutex, oneshot};
use tokio_util::sync::CancellationToken;

use crate::messages::default_convert_to_llm_fn;
use crate::types::{
    AgentEvent, AgentMessage, AgentState, AgentThinkingLevel, ConvertToLlmFn, QueueMode, StreamFn, ToolExecutionMode,
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
    session_id: Arc<ParkingMutex<Option<String>>>,
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
            session_id: Arc::new(ParkingMutex::new(options.session_id)),
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
        self.session_id.lock().clone()
    }

    pub fn set_session_id(&self, session_id: Option<String>) {
        *self.session_id.lock() = session_id;
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
        // Avoid nested mutex await while holding `active_run` (can stall finish_run).
        let rx = {
            let guard = self.active_run.lock().await;
            match guard.as_ref() {
                Some(run) => run.idle_rx.try_lock().ok().and_then(|mut slot| slot.take()),
                None => None,
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
}
