//! Agent loop configuration and provider stream function wiring.

use std::sync::{Arc, Mutex as StdMutex};

use elph_ai::{ProviderResponse, SimpleStreamOptions};

use crate::harness::types::{
    AfterProviderResponseEvent, BeforeProviderPayloadEvent, BeforeProviderRequestEvent, ContextEvent, ToolCallEvent,
    ToolResultEvent, clone_stream_options,
};
use crate::mode::{plan_mode_block_reason, plan_mode_blocks_tool};
use crate::runtime::try_block_on;
use crate::types::{
    AfterToolCallResult, AgentContext, AgentLoopConfig, AgentLoopTurnUpdate, AgentMessage, BeforeToolCallResult,
    GetQueuedMessagesFn, PrepareNextTurnFn, StreamFn,
};

use super::super::helpers::merge_harness_into_simple;
use super::super::{AgentHarness, AgentHarnessTurnState};

impl<S> AgentHarness<S>
where
    S: crate::session::types::SessionStorage + Clone + Send + Sync + 'static,
    S::Metadata: crate::session::types::HasSessionId + Send + Sync,
{
    pub(super) fn create_context(
        &self,
        turn_state: &AgentHarnessTurnState,
        system_prompt: Option<&str>,
    ) -> AgentContext {
        AgentContext {
            system_prompt: system_prompt.unwrap_or(&turn_state.system_prompt).to_string(),
            messages: turn_state.messages.clone(),
            tools: turn_state.active_tools.clone(),
        }
    }

    pub(super) fn create_loop_config(&self, turn_state: Arc<StdMutex<AgentHarnessTurnState>>) -> AgentLoopConfig {
        let shared = self.shared.clone();
        let snapshot = turn_state.lock().expect("turn state lock");
        let thinking_level = snapshot.thinking_level;
        let model = snapshot.model.clone();
        drop(snapshot);

        let get_steering: GetQueuedMessagesFn = {
            let shared = shared.clone();
            Arc::new(move || {
                let shared = shared.clone();
                Box::pin(async move {
                    let harness = AgentHarness { shared };
                    harness.drain_queued_messages(true).await
                })
            })
        };
        let get_follow_up: GetQueuedMessagesFn = {
            let shared = shared.clone();
            Arc::new(move || {
                let shared = shared.clone();
                Box::pin(async move {
                    let harness = AgentHarness { shared };
                    harness.drain_queued_messages(false).await
                })
            })
        };

        let prepare_shared = shared.clone();
        let prepare_turn_state = turn_state.clone();
        let prepare_next_turn: Option<PrepareNextTurnFn> = Some(Arc::new(move |_| {
            let shared = prepare_shared.clone();
            let turn_state = prepare_turn_state.clone();
            Box::pin(async move {
                let harness = AgentHarness { shared };
                harness.flush_pending_session_writes().await.ok()?;
                let next = harness.create_turn_state().await.ok()?;
                *turn_state.lock().expect("turn state lock") = next;
                let snapshot = turn_state.lock().expect("turn state lock");
                Some(AgentLoopTurnUpdate {
                    context: Some(harness.create_context(&snapshot, None)),
                    model: Some(snapshot.model.clone()),
                    thinking_level: Some(snapshot.thinking_level),
                })
            })
        }));

        let hooks = Arc::new(self.shared.hooks.clone_shallow());
        let plan_shared = shared.clone();
        let before_tool_call: Option<crate::types::BeforeToolCallFn> =
            Some(Arc::new(move |ctx: crate::types::BeforeToolCallContext, _| {
                let hooks = hooks.clone();
                let plan_shared = plan_shared.clone();
                let tool_name = ctx.tool_call.name.clone();
                let event = ToolCallEvent {
                    tool_call_id: ctx.tool_call.id.clone(),
                    tool_name: tool_name.clone(),
                    input: ctx.args.clone(),
                };
                Box::pin(async move {
                    let mode = *plan_shared.collaboration_mode.lock().await;
                    if plan_mode_blocks_tool(mode, &tool_name) {
                        return Some(BeforeToolCallResult {
                            block: true,
                            reason: Some(plan_mode_block_reason(&tool_name)),
                            args: None,
                        });
                    }
                    let result = hooks.emit_tool_call(&event).await.ok()??;
                    Some(BeforeToolCallResult {
                        block: result.block,
                        reason: result.reason,
                        args: None,
                    })
                })
            }));
        let hooks = Arc::new(self.shared.hooks.clone_shallow());
        let after_tool_call: Option<crate::types::AfterToolCallFn> =
            Some(Arc::new(move |ctx: crate::types::AfterToolCallContext, _| {
                let hooks = hooks.clone();
                let event = ToolResultEvent {
                    tool_call_id: ctx.tool_call.id.clone(),
                    tool_name: ctx.tool_call.name.clone(),
                    input: ctx.args.clone(),
                    content: ctx.result.content.clone(),
                    details: ctx.result.details.clone(),
                    is_error: ctx.is_error,
                };
                Box::pin(async move {
                    let result = hooks.emit_tool_result(&event).await.ok()??;
                    Some(AfterToolCallResult {
                        content: result.content,
                        details: result.details,
                        is_error: result.is_error,
                        terminate: result.terminate,
                    })
                })
            }));

        let transform_shared = shared.clone();
        let transform_context: Option<crate::types::TransformContextFn> =
            Some(Arc::new(move |messages: Vec<AgentMessage>, _| {
                let shared = transform_shared.clone();
                Box::pin(async move {
                    let harness = AgentHarness { shared };
                    let event = ContextEvent {
                        messages: messages.clone(),
                    };
                    match harness.shared.hooks.emit_context(&event).await {
                        Ok(Some(result)) => Ok(result.messages),
                        Ok(None) => Ok(messages),
                        Err(error) => Err(error.to_string()),
                    }
                })
            }));

        let stream_options = SimpleStreamOptions {
            base: Default::default(),
            reasoning: thinking_level.to_stream_reasoning(),
            thinking_budgets: None,
        };

        AgentLoopConfig {
            model,
            stream_options,
            convert_to_llm: self.shared.convert_to_llm.clone(),
            transform_context,
            get_api_key: None,
            should_stop_after_turn: None,
            prepare_next_turn,
            get_steering_messages: Some(get_steering),
            get_follow_up_messages: Some(get_follow_up),
            tool_execution: crate::types::ToolExecutionMode::Parallel,
            before_tool_call,
            after_tool_call,
            stream_fn: Some(self.create_stream_fn(turn_state)),
        }
    }

    fn create_stream_fn(&self, turn_state: Arc<StdMutex<AgentHarnessTurnState>>) -> StreamFn {
        let models = self.shared.models.clone();
        let hooks = self.shared.hooks.clone();
        Arc::new(move |model, context, options| {
            let (mut snapshot, session_id) = {
                let turn_state = turn_state.lock().expect("turn state lock");
                (
                    clone_stream_options(&turn_state.stream_options),
                    turn_state.session_id.clone(),
                )
            };

            if let Ok(Ok(merged)) = try_block_on(hooks.emit_before_provider_request(&BeforeProviderRequestEvent {
                model: model.clone(),
                session_id: session_id.clone(),
                stream_options: clone_stream_options(&snapshot),
            })) {
                snapshot = merged;
            }

            let hooks_for_payload = hooks.clone();
            let mut simple = merge_harness_into_simple(options, &snapshot, &session_id);
            let existing_on_payload = simple.base.on_payload.take();
            simple.base.on_payload = Some(Arc::new(move |payload, model_ref| {
                let hooks = hooks_for_payload.clone();
                let existing = existing_on_payload.clone();
                Box::pin(async move {
                    let mut current = payload;
                    if let Ok(transformed) = hooks
                        .emit_before_provider_payload(&BeforeProviderPayloadEvent {
                            model: model_ref.clone(),
                            payload: current.clone(),
                        })
                        .await
                    {
                        current = transformed;
                    }
                    if let Some(previous) = existing {
                        let input = current.clone();
                        if let Some(transformed) = previous(input, model_ref).await {
                            current = transformed;
                        }
                    }
                    Some(current)
                })
            }));

            let hooks_for_response = hooks.clone();
            let existing_on_response = simple.base.on_response.take();
            simple.base.on_response = Some(Arc::new(move |response: ProviderResponse, model_ref| {
                let hooks = hooks_for_response.clone();
                let existing = existing_on_response.clone();
                Box::pin(async move {
                    hooks
                        .emit_after_provider_response(&AfterProviderResponseEvent {
                            status: response.status,
                            headers: response.headers.clone(),
                        })
                        .await;
                    if let Some(previous) = existing {
                        previous(response, model_ref).await;
                    }
                })
            }));

            models.stream_simple(model, context, Some(simple))
        })
    }
}
