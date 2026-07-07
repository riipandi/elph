//! Hook registry for typed `AgentHarness` events.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use crate::harness::types::{
    AgentHarnessError, AgentHarnessErrorCode, AgentHarnessOwnEvent, BeforeAgentStartEvent, BeforeAgentStartResult,
    BeforeProviderPayloadEvent, BeforeProviderPayloadResult, BeforeProviderRequestEvent, BeforeProviderRequestResult,
    ContextEvent, ContextResult, SessionBeforeCompactEvent, SessionBeforeCompactResult, SessionBeforeTreeEvent,
    SessionBeforeTreeResult, ToolCallEvent, ToolCallHookResult, ToolResultEvent, ToolResultPatch,
};
use crate::types::AgentEvent;

pub const SUBSCRIBER_EVENT_TYPE: &str = "*";

/// Combined harness event delivered to `subscribe` listeners.
#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum AgentHarnessEvent {
    Agent(AgentEvent),
    Own(AgentHarnessOwnEvent),
}

pub type HarnessSubscriber =
    Arc<dyn Fn(AgentHarnessEvent, Option<CancellationToken>) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>;

type BeforeAgentStartHandler = Arc<
    dyn Fn(&BeforeAgentStartEvent) -> Pin<Box<dyn Future<Output = Option<BeforeAgentStartResult>> + Send>>
        + Send
        + Sync,
>;
type ContextHandler = Arc<
    dyn Fn(
            &ContextEvent,
        )
            -> Pin<Box<dyn Future<Output = std::result::Result<Option<ContextResult>, AgentHarnessError>> + Send>>
        + Send
        + Sync,
>;
type BeforeProviderRequestHandler = Arc<
    dyn Fn(&BeforeProviderRequestEvent) -> Pin<Box<dyn Future<Output = Option<BeforeProviderRequestResult>> + Send>>
        + Send
        + Sync,
>;
type BeforeProviderPayloadHandler = Arc<
    dyn Fn(&BeforeProviderPayloadEvent) -> Pin<Box<dyn Future<Output = Option<BeforeProviderPayloadResult>> + Send>>
        + Send
        + Sync,
>;
type ToolCallHandler =
    Arc<dyn Fn(&ToolCallEvent) -> Pin<Box<dyn Future<Output = Option<ToolCallHookResult>> + Send>> + Send + Sync>;
type ToolResultHandler =
    Arc<dyn Fn(&ToolResultEvent) -> Pin<Box<dyn Future<Output = Option<ToolResultPatch>> + Send>> + Send + Sync>;
type SessionBeforeCompactHandler = Arc<
    dyn Fn(&SessionBeforeCompactEvent) -> Pin<Box<dyn Future<Output = Option<SessionBeforeCompactResult>> + Send>>
        + Send
        + Sync,
>;
type SessionBeforeTreeHandler = Arc<
    dyn Fn(&SessionBeforeTreeEvent) -> Pin<Box<dyn Future<Output = Option<SessionBeforeTreeResult>> + Send>>
        + Send
        + Sync,
>;

#[derive(Default)]
struct TypedHandlers {
    before_agent_start: Vec<BeforeAgentStartHandler>,
    context: Vec<ContextHandler>,
    before_provider_request: Vec<BeforeProviderRequestHandler>,
    before_provider_payload: Vec<BeforeProviderPayloadHandler>,
    tool_call: Vec<ToolCallHandler>,
    tool_result: Vec<ToolResultHandler>,
    session_before_compact: Vec<SessionBeforeCompactHandler>,
    session_before_tree: Vec<SessionBeforeTreeHandler>,
}

#[derive(Clone)]
pub struct HookRegistry {
    subscribers: Arc<Mutex<Vec<HarnessSubscriber>>>,
    typed: Arc<Mutex<TypedHandlers>>,
    named: Arc<Mutex<HashMap<String, Vec<HarnessSubscriber>>>>,
}

impl Default for HookRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl HookRegistry {
    pub fn new() -> Self {
        Self {
            subscribers: Arc::new(Mutex::new(Vec::new())),
            typed: Arc::new(Mutex::new(TypedHandlers::default())),
            named: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn clone_shallow(&self) -> Self {
        self.clone()
    }

    pub async fn subscribe(&self, listener: HarnessSubscriber) -> usize {
        let mut subscribers = self.subscribers.lock().await;
        subscribers.push(listener);
        subscribers.len() - 1
    }

    pub async fn on(&self, event_type: &str, listener: HarnessSubscriber) -> usize {
        let mut named = self.named.lock().await;
        let handlers = named.entry(event_type.to_string()).or_default();
        handlers.push(listener);
        handlers.len() - 1
    }

    pub async fn register_before_agent_start(&self, handler: BeforeAgentStartHandler) -> usize {
        let mut typed = self.typed.lock().await;
        typed.before_agent_start.push(handler);
        typed.before_agent_start.len() - 1
    }

    pub async fn register_context(&self, handler: ContextHandler) -> usize {
        let mut typed = self.typed.lock().await;
        typed.context.push(handler);
        typed.context.len() - 1
    }

    pub async fn register_tool_call(&self, handler: ToolCallHandler) -> usize {
        let mut typed = self.typed.lock().await;
        typed.tool_call.push(handler);
        typed.tool_call.len() - 1
    }

    pub async fn register_tool_result(&self, handler: ToolResultHandler) -> usize {
        let mut typed = self.typed.lock().await;
        typed.tool_result.push(handler);
        typed.tool_result.len() - 1
    }

    pub async fn emit_subscriber(
        &self,
        event: AgentHarnessEvent,
        signal: Option<CancellationToken>,
    ) -> std::result::Result<(), AgentHarnessError> {
        let subscribers = self.subscribers.lock().await.clone();
        for listener in subscribers.iter() {
            listener(event.clone(), signal.clone()).await;
        }
        Ok(())
    }

    pub async fn emit_before_agent_start(
        &self,
        event: &BeforeAgentStartEvent,
    ) -> std::result::Result<Option<BeforeAgentStartResult>, AgentHarnessError> {
        let typed = self.typed.lock().await;
        let mut last = None;
        for handler in &typed.before_agent_start {
            if let Some(result) = handler(event).await {
                last = Some(result);
            }
        }
        Ok(last)
    }

    pub async fn emit_context(
        &self,
        event: &ContextEvent,
    ) -> std::result::Result<Option<ContextResult>, AgentHarnessError> {
        let handlers = self.typed.lock().await.context.clone();
        let mut last = None;
        for handler in &handlers {
            if let Some(result) = handler(event).await? {
                last = Some(result);
            }
        }
        Ok(last)
    }

    pub async fn emit_before_provider_request(
        &self,
        event: &BeforeProviderRequestEvent,
    ) -> std::result::Result<Option<BeforeProviderRequestResult>, AgentHarnessError> {
        let typed = self.typed.lock().await;
        let mut last = None;
        for handler in &typed.before_provider_request {
            if let Some(result) = handler(event).await {
                last = Some(result);
            }
        }
        Ok(last)
    }

    pub async fn emit_before_provider_payload(
        &self,
        event: &BeforeProviderPayloadEvent,
    ) -> std::result::Result<Option<BeforeProviderPayloadResult>, AgentHarnessError> {
        let typed = self.typed.lock().await;
        let mut last = None;
        for handler in &typed.before_provider_payload {
            if let Some(result) = handler(event).await {
                last = Some(result);
            }
        }
        Ok(last)
    }

    pub async fn emit_tool_call(
        &self,
        event: &ToolCallEvent,
    ) -> std::result::Result<Option<ToolCallHookResult>, AgentHarnessError> {
        let typed = self.typed.lock().await;
        let mut last = None;
        for handler in &typed.tool_call {
            if let Some(result) = handler(event).await {
                last = Some(result);
            }
        }
        Ok(last)
    }

    pub async fn emit_tool_result(
        &self,
        event: &ToolResultEvent,
    ) -> std::result::Result<Option<ToolResultPatch>, AgentHarnessError> {
        let typed = self.typed.lock().await;
        let mut last = None;
        for handler in &typed.tool_result {
            if let Some(result) = handler(event).await {
                last = Some(result);
            }
        }
        Ok(last)
    }

    pub async fn emit_session_before_compact(
        &self,
        event: &SessionBeforeCompactEvent,
    ) -> std::result::Result<Option<SessionBeforeCompactResult>, AgentHarnessError> {
        let typed = self.typed.lock().await;
        let mut last = None;
        for handler in &typed.session_before_compact {
            if let Some(result) = handler(event).await {
                last = Some(result);
            }
        }
        Ok(last)
    }

    pub async fn emit_session_before_tree(
        &self,
        event: &SessionBeforeTreeEvent,
    ) -> std::result::Result<Option<SessionBeforeTreeResult>, AgentHarnessError> {
        let typed = self.typed.lock().await;
        let mut last = None;
        for handler in &typed.session_before_tree {
            if let Some(result) = handler(event).await {
                last = Some(result);
            }
        }
        Ok(last)
    }
}

pub fn normalize_hook_error(error: impl std::fmt::Display) -> AgentHarnessError {
    AgentHarnessError::new(AgentHarnessErrorCode::Hook, error.to_string())
}

pub fn normalize_harness_error(error: impl std::fmt::Display, fallback: AgentHarnessErrorCode) -> AgentHarnessError {
    let message = error.to_string();
    if message.contains("session") {
        return AgentHarnessError::new(AgentHarnessErrorCode::Session, message);
    }
    AgentHarnessError::new(fallback, message)
}
