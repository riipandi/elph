//! Agent harness hook and event subscription registration.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use tokio_util::sync::CancellationToken;

use crate::harness::hooks::{AgentHarnessEvent, HookRegistry};
use crate::harness::types::{
    AfterProviderResponseEvent, BeforeAgentStartEvent, ContextEvent, SessionBeforeCompactEvent, SessionBeforeTreeEvent,
    ToolCallEvent, ToolResultEvent,
};

use super::{AgentHarness, HarnessOpResult};

impl<S> AgentHarness<S>
where
    S: crate::session::types::SessionStorage + Clone + Send + Sync + 'static,
    S::Metadata: crate::session::types::HasSessionId + Send + Sync,
{
    pub async fn subscribe<F, Fut>(&self, listener: F) -> usize
    where
        F: Fn(AgentHarnessEvent, Option<CancellationToken>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let listener = Arc::new(move |event, signal| {
            let fut = listener(event, signal);
            Box::pin(fut) as Pin<Box<dyn Future<Output = ()> + Send>>
        });
        self.shared.hooks.subscribe(listener).await
    }

    pub async fn on_before_agent_start<F, Fut>(&self, handler: F) -> usize
    where
        F: Fn(&BeforeAgentStartEvent) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Option<crate::harness::types::BeforeAgentStartResult>> + Send + 'static,
    {
        let handler = Arc::new(move |event: &BeforeAgentStartEvent| {
            let fut = handler(event);
            Box::pin(fut) as Pin<Box<dyn Future<Output = Option<crate::harness::types::BeforeAgentStartResult>> + Send>>
        });
        self.shared.hooks.register_before_agent_start(handler).await
    }

    pub async fn on_context<F, Fut>(&self, handler: F) -> usize
    where
        F: Fn(&ContextEvent) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = HarnessOpResult<Option<crate::harness::types::ContextResult>>> + Send + 'static,
    {
        let handler = Arc::new(move |event: &ContextEvent| {
            let fut = handler(event);
            Box::pin(fut)
                as Pin<Box<dyn Future<Output = HarnessOpResult<Option<crate::harness::types::ContextResult>>> + Send>>
        });
        self.shared.hooks.register_context(handler).await
    }

    pub async fn on_tool_call<F, Fut>(&self, handler: F) -> usize
    where
        F: Fn(&ToolCallEvent) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Option<crate::harness::types::ToolCallHookResult>> + Send + 'static,
    {
        let handler = Arc::new(move |event: &ToolCallEvent| {
            let fut = handler(event);
            Box::pin(fut) as Pin<Box<dyn Future<Output = Option<crate::harness::types::ToolCallHookResult>> + Send>>
        });
        self.shared.hooks.register_tool_call(handler).await
    }

    pub async fn on_tool_result<F, Fut>(&self, handler: F) -> usize
    where
        F: Fn(&ToolResultEvent) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Option<crate::harness::types::ToolResultPatch>> + Send + 'static,
    {
        let handler = Arc::new(move |event: &ToolResultEvent| {
            let fut = handler(event);
            Box::pin(fut) as Pin<Box<dyn Future<Output = Option<crate::harness::types::ToolResultPatch>> + Send>>
        });
        self.shared.hooks.register_tool_result(handler).await
    }

    pub async fn on_before_provider_request<F, Fut>(&self, handler: F) -> usize
    where
        F: Fn(&crate::harness::types::BeforeProviderRequestEvent) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Option<crate::harness::types::BeforeProviderRequestResult>> + Send + 'static,
    {
        let handler = Arc::new(move |event: &crate::harness::types::BeforeProviderRequestEvent| {
            let fut = handler(event);
            Box::pin(fut)
                as Pin<Box<dyn Future<Output = Option<crate::harness::types::BeforeProviderRequestResult>> + Send>>
        });
        self.shared.hooks.register_before_provider_request(handler).await
    }

    pub async fn on_before_provider_payload<F, Fut>(&self, handler: F) -> usize
    where
        F: Fn(&crate::harness::types::BeforeProviderPayloadEvent) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Option<crate::harness::types::BeforeProviderPayloadResult>> + Send + 'static,
    {
        let handler = Arc::new(move |event: &crate::harness::types::BeforeProviderPayloadEvent| {
            let fut = handler(event);
            Box::pin(fut)
                as Pin<Box<dyn Future<Output = Option<crate::harness::types::BeforeProviderPayloadResult>> + Send>>
        });
        self.shared.hooks.register_before_provider_payload(handler).await
    }

    pub async fn on_session_before_compact<F, Fut>(&self, handler: F) -> usize
    where
        F: Fn(&SessionBeforeCompactEvent) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Option<crate::harness::types::SessionBeforeCompactResult>> + Send + 'static,
    {
        let handler = Arc::new(move |event: &SessionBeforeCompactEvent| {
            let fut = handler(event);
            Box::pin(fut)
                as Pin<Box<dyn Future<Output = Option<crate::harness::types::SessionBeforeCompactResult>> + Send>>
        });
        self.shared.hooks.register_session_before_compact(handler).await
    }

    pub async fn on_session_before_tree<F, Fut>(&self, handler: F) -> usize
    where
        F: Fn(&SessionBeforeTreeEvent) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Option<crate::harness::types::SessionBeforeTreeResult>> + Send + 'static,
    {
        let handler = Arc::new(move |event: &SessionBeforeTreeEvent| {
            let fut = handler(event);
            Box::pin(fut)
                as Pin<Box<dyn Future<Output = Option<crate::harness::types::SessionBeforeTreeResult>> + Send>>
        });
        self.shared.hooks.register_session_before_tree(handler).await
    }

    pub async fn on_after_provider_response<F, Fut>(&self, handler: F) -> usize
    where
        F: Fn(&AfterProviderResponseEvent) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let handler = Arc::new(move |event: &AfterProviderResponseEvent| {
            let fut = handler(event);
            Box::pin(fut) as Pin<Box<dyn Future<Output = ()> + Send>>
        });
        self.shared.hooks.register_after_provider_response(handler).await
    }

    /// Upstream-compatible hook registration by snake_case event name.
    ///
    /// Mutation hooks return an optional [`HarnessHookResult`] to alter harness behavior.
    /// Observe-only hooks (for example `session_compact`, `model_update`) may return `None`.
    pub async fn on<F, Fut>(&self, event_type: &str, handler: F) -> HarnessOpResult<usize>
    where
        F: Fn(crate::harness::types::AgentHarnessOwnEvent) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Option<crate::harness::types::HarnessHookResult>> + Send + 'static,
    {
        crate::harness::generic_on::register_generic_on(self, event_type, handler).await
    }

    pub(crate) fn hook_registry(&self) -> &HookRegistry {
        &self.shared.hooks
    }
}
