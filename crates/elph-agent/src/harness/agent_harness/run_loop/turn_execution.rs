//! Single-turn agent loop execution.

use std::sync::{Arc, Mutex as StdMutex};

use elph_ai::{AssistantMessage, Message, Model, UserContent};
use tokio_util::sync::CancellationToken;

use crate::agent_loop::run_agent_loop;
use crate::goals::{GoalRuntime, GoalTurnFinish, GoalTurnStart};
use crate::harness::types::{
    AgentHarnessError, AgentHarnessErrorCode, AgentHarnessPromptOptions, BeforeAgentStartEvent,
};
use crate::types::{AgentEvent, llm_message_to_agent};

use super::super::helpers::{create_failure_message, create_user_message, now_ms};
use super::super::{AgentHarness, AgentHarnessTurnState, HarnessOpResult};

impl<S> AgentHarness<S>
where
    S: crate::session::types::SessionStorage + Clone + Send + Sync + 'static,
    S::Metadata: crate::session::types::HasSessionId + Send + Sync,
{
    async fn emit_run_failure(
        &self,
        model: &Model,
        error: &str,
        aborted: bool,
        _emit: &crate::agent_loop::AgentEventCallback,
    ) -> HarnessOpResult<AssistantMessage> {
        let failure_message = llm_message_to_agent(create_failure_message(model, error, aborted));
        self.handle_agent_event(
            AgentEvent::MessageStart {
                message: failure_message.clone(),
            },
            None,
        )
        .await?;
        self.handle_agent_event(
            AgentEvent::MessageEnd {
                message: failure_message.clone(),
            },
            None,
        )
        .await?;
        self.handle_agent_event(
            AgentEvent::TurnEnd {
                message: failure_message.clone(),
                tool_results: Vec::new(),
            },
            None,
        )
        .await?;
        self.handle_agent_event(
            AgentEvent::AgentEnd {
                messages: vec![failure_message.clone()],
            },
            None,
        )
        .await?;
        self.flush_pending_session_writes().await?;
        match failure_message.as_llm() {
            Some(Message::Assistant(assistant)) => Ok(assistant.clone()),
            _ => Err(AgentHarnessError::new(
                AgentHarnessErrorCode::InvalidState,
                "Failure message was not an assistant message",
            )),
        }
    }

    #[cfg_attr(feature = "tracing", fastrace::trace(name = "elph.agent.execute_turn"))]
    pub(in crate::harness::agent_harness) async fn execute_turn(
        &self,
        turn_state: AgentHarnessTurnState,
        text: String,
        options: Option<AgentHarnessPromptOptions>,
    ) -> HarnessOpResult<AssistantMessage> {
        let images = options.as_ref().and_then(|o| o.images.clone());
        let mut messages = vec![create_user_message(text.clone(), images.clone())];

        if !self.shared.next_turn_queue.lock().await.is_empty() {
            let queued = self.shared.next_turn_queue.lock().await.drain(..).collect::<Vec<_>>();
            if let Err(error) = self.emit_queue_update().await {
                *self.shared.next_turn_queue.lock().await = queued;
                return Err(error);
            }
            let prompt = messages.pop().expect("prompt message");
            messages = queued;
            messages.push(prompt);
        }

        let before_result = self
            .shared
            .hooks
            .emit_before_agent_start(&BeforeAgentStartEvent {
                prompt: text,
                images: images.clone(),
                system_prompt: turn_state.system_prompt.clone(),
                resources: turn_state.resources.clone(),
            })
            .await?;

        if let Some(extra) = before_result.as_ref().and_then(|r| r.messages.clone()) {
            messages.extend(extra);
        }

        let abort_token = {
            let guard = self.shared.active_run.lock().await;
            guard
                .as_ref()
                .map(|run| run.abort_token.clone())
                .unwrap_or_else(CancellationToken::new)
        };

        if let Some(goal_runtime) = &self.shared.goal_runtime {
            let mode = *self.shared.collaboration_mode.lock().await;
            match goal_runtime.start_turn(mode).await {
                Ok(GoalTurnStart::Ok) => {}
                Ok(GoalTurnStart::Blocked(message)) => {
                    return Err(AgentHarnessError::new(AgentHarnessErrorCode::InvalidState, message));
                }
                Err(error) => {
                    return Err(AgentHarnessError::new(AgentHarnessErrorCode::InvalidState, error.to_string()));
                }
            }
        }

        let turn_state = Arc::new(StdMutex::new(turn_state));
        let system_prompt_override = before_result.and_then(|r| r.system_prompt);
        let context =
            self.create_context(&turn_state.lock().expect("turn state lock"), system_prompt_override.as_deref());
        let config = self.create_loop_config(turn_state.clone());
        let shared = self.shared.clone();

        let emit_token = abort_token.clone();
        let emit: crate::agent_loop::AgentEventCallback = Arc::new(move |event| {
            let shared = shared.clone();
            let token = emit_token.clone();
            Box::pin(async move {
                let harness = AgentHarness { shared: shared.clone() };
                let _ = harness.handle_agent_event(event, Some(token)).await;
            })
        });

        let run_result = match run_agent_loop(messages, context, config, emit.clone(), Some(abort_token.clone())).await
        {
            Ok(messages) => messages,
            Err(error) => {
                let model = turn_state.lock().expect("turn state lock").model.clone();
                return self
                    .emit_run_failure(&model, &error, abort_token.is_cancelled(), &emit)
                    .await;
            }
        };

        self.flush_pending_session_writes().await?;

        for message in run_result.into_iter().rev() {
            if let Some(assistant) = message.as_llm()
                && let Message::Assistant(assistant) = assistant
            {
                if let Some(goal_runtime) = &self.shared.goal_runtime {
                    let mode = *self.shared.collaboration_mode.lock().await;
                    match goal_runtime.finish_turn(mode, Some(&assistant.usage)).await {
                        Ok(GoalTurnFinish::BudgetLimited(goal)) => {
                            let steering = GoalRuntime::budget_steering(&goal);
                            self.shared
                                .next_turn_queue
                                .lock()
                                .await
                                .push(llm_message_to_agent(Message::User {
                                    content: UserContent::Text(steering),
                                    timestamp: now_ms(),
                                }));
                        }
                        Ok(GoalTurnFinish::Continuation(goal)) => {
                            let steering = GoalRuntime::continuation_steering(&goal);
                            self.shared
                                .next_turn_queue
                                .lock()
                                .await
                                .push(llm_message_to_agent(Message::User {
                                    content: UserContent::Text(steering),
                                    timestamp: now_ms(),
                                }));
                        }
                        Ok(GoalTurnFinish::None) => {}
                        Err(error) => {
                            return Err(AgentHarnessError::new(AgentHarnessErrorCode::InvalidState, error.to_string()));
                        }
                    }
                }
                return Ok(assistant.clone());
            }
        }

        Err(AgentHarnessError::new(
            AgentHarnessErrorCode::InvalidState,
            "AgentHarness prompt completed without an assistant message",
        ))
    }
}
