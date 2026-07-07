//! Upstream-compatible generic `AgentHarness::on(event_type, handler)` registration.

use std::future::Future;
use std::sync::Arc;

use crate::harness::agent_harness::{AgentHarness, HarnessOpResult};
use crate::harness::hooks::AgentHarnessEvent;
use crate::harness::types::{
    AgentHarnessError, AgentHarnessErrorCode, AgentHarnessOwnEvent, HarnessHookResult, is_known_harness_hook_type,
};
use crate::session::types::{HasSessionId, SessionStorage};

fn wrong_result_type(event_type: &str) -> AgentHarnessError {
    AgentHarnessError::new(
        AgentHarnessErrorCode::Hook,
        format!("handler for `{event_type}` returned the wrong result type"),
    )
}

pub async fn register_generic_on<S, F, Fut>(
    harness: &AgentHarness<S>,
    event_type: &str,
    handler: F,
) -> HarnessOpResult<usize>
where
    S: SessionStorage + Clone + Send + Sync + 'static,
    S::Metadata: HasSessionId + Send + Sync,
    F: Fn(AgentHarnessOwnEvent) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Option<HarnessHookResult>> + Send + 'static,
{
    if !is_known_harness_hook_type(event_type) {
        return Err(AgentHarnessError::new(
            AgentHarnessErrorCode::Hook,
            format!("Unknown harness hook type: {event_type}"),
        ));
    }

    let handler = Arc::new(handler);

    match event_type {
        "before_agent_start" => {
            let h = handler.clone();
            Ok(harness
                .on_before_agent_start(move |event| {
                    let h = h.clone();
                    let event = event.clone();
                    async move {
                        match h(AgentHarnessOwnEvent::BeforeAgentStart(event)).await {
                            Some(HarnessHookResult::BeforeAgentStart(result)) => Some(result),
                            None => None,
                            Some(_) => None,
                        }
                    }
                })
                .await)
        }
        "context" => {
            let h = handler.clone();
            Ok(harness
                .on_context(move |event| {
                    let h = h.clone();
                    let event = event.clone();
                    async move {
                        match h(AgentHarnessOwnEvent::Context(event)).await {
                            Some(HarnessHookResult::Context(result)) => Ok(Some(result)),
                            None => Ok(None),
                            Some(_) => Err(wrong_result_type("context")),
                        }
                    }
                })
                .await)
        }
        "before_provider_request" => {
            let h = handler.clone();
            Ok(harness
                .on_before_provider_request(move |event| {
                    let h = h.clone();
                    let event = event.clone();
                    async move {
                        match h(AgentHarnessOwnEvent::BeforeProviderRequest(event)).await {
                            Some(HarnessHookResult::BeforeProviderRequest(result)) => Some(result),
                            None => None,
                            Some(_) => None,
                        }
                    }
                })
                .await)
        }
        "before_provider_payload" => {
            let h = handler.clone();
            Ok(harness
                .on_before_provider_payload(move |event| {
                    let h = h.clone();
                    let event = event.clone();
                    async move {
                        match h(AgentHarnessOwnEvent::BeforeProviderPayload(event)).await {
                            Some(HarnessHookResult::BeforeProviderPayload(result)) => Some(result),
                            None => None,
                            Some(_) => None,
                        }
                    }
                })
                .await)
        }
        "after_provider_response" => {
            let h = handler.clone();
            Ok(harness
                .on_after_provider_response(move |event| {
                    let h = h.clone();
                    let event = event.clone();
                    async move {
                        let _ = h(AgentHarnessOwnEvent::AfterProviderResponse(event)).await;
                    }
                })
                .await)
        }
        "tool_call" => {
            let h = handler.clone();
            Ok(harness
                .on_tool_call(move |event| {
                    let h = h.clone();
                    let event = event.clone();
                    async move {
                        match h(AgentHarnessOwnEvent::ToolCall(event)).await {
                            Some(HarnessHookResult::ToolCall(result)) => Some(result),
                            None => None,
                            Some(_) => None,
                        }
                    }
                })
                .await)
        }
        "tool_result" => {
            let h = handler.clone();
            Ok(harness
                .on_tool_result(move |event| {
                    let h = h.clone();
                    let event = event.clone();
                    async move {
                        match h(AgentHarnessOwnEvent::ToolResult(event)).await {
                            Some(HarnessHookResult::ToolResult(result)) => Some(result),
                            None => None,
                            Some(_) => None,
                        }
                    }
                })
                .await)
        }
        "session_before_compact" => {
            let h = handler.clone();
            Ok(harness
                .on_session_before_compact(move |event| {
                    let h = h.clone();
                    let event = event.clone();
                    async move {
                        match h(AgentHarnessOwnEvent::SessionBeforeCompact(event)).await {
                            Some(HarnessHookResult::SessionBeforeCompact(result)) => Some(result),
                            None => None,
                            Some(_) => None,
                        }
                    }
                })
                .await)
        }
        "session_before_tree" => {
            let h = handler.clone();
            Ok(harness
                .on_session_before_tree(move |event| {
                    let h = h.clone();
                    let event = event.clone();
                    async move {
                        match h(AgentHarnessOwnEvent::SessionBeforeTree(event)).await {
                            Some(HarnessHookResult::SessionBeforeTree(result)) => Some(result),
                            None => None,
                            Some(_) => None,
                        }
                    }
                })
                .await)
        }
        hook_type => {
            let hook_type = Arc::new(hook_type.to_string());
            let hook_type_for_registry = hook_type.clone();
            let h = handler.clone();
            Ok(harness
                .hook_registry()
                .on(
                    hook_type_for_registry.as_str(),
                    Arc::new(move |event, _signal| {
                        let h = h.clone();
                        let hook_type = hook_type.clone();
                        Box::pin(async move {
                            if let AgentHarnessEvent::Own(own) = event
                                && own.hook_type() == hook_type.as_str()
                            {
                                let _ = h(own).await;
                            }
                        })
                    }),
                )
                .await)
        }
    }
}
