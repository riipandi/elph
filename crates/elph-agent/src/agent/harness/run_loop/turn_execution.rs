//! Single-turn agent loop execution.

use std::sync::{Arc, Mutex as StdMutex};

use elph_ai::{AssistantMessage, Message, Model, UserContent};
use tokio_util::sync::CancellationToken;

use crate::agent::harness::types::AgentHarnessError;
use crate::agent::harness::types::AgentHarnessErrorCode;
use crate::agent::harness::types::AgentHarnessPromptOptions;
use crate::agent::harness::types::BeforeAgentStartEvent;
use crate::goals::{GoalRuntime, GoalTurnFinish, GoalTurnStart};
use crate::runtime::run_agent_loop;
use crate::types::AgentEvent;
use crate::types::llm_message_to_agent;

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
        _emit: &crate::runtime::AgentEventCallback,
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
    pub(in crate::agent::harness) async fn execute_turn(
        &self,
        turn_state: AgentHarnessTurnState,
        text: String,
        options: Option<AgentHarnessPromptOptions>,
        operation_id: String,
    ) -> HarnessOpResult<AssistantMessage> {
        let images = options.as_ref().and_then(|o| o.images.clone());
        let mut messages = vec![create_user_message(text.clone(), images.clone())];
        let mut consumed_queue_ids = Vec::new();

        if !self.shared.next_turn_queue.lock().await.is_empty() {
            let queued_items = self.shared.next_turn_queue.lock().await.drain(..).collect::<Vec<_>>();
            if let Err(error) = self.emit_queue_update().await {
                *self.shared.next_turn_queue.lock().await = queued_items;
                return Err(error);
            }
            let ids: Vec<String> = queued_items.iter().map(|(id, _)| id.clone()).collect();
            let queued: Vec<_> = queued_items.into_iter().map(|(_, m)| m).collect();
            let _ = self
                .journal_queue_consume(crate::session::durability::QueueKind::NextTurn, ids.clone(), None)
                .await;
            consumed_queue_ids = ids;
            let prompt = messages.pop().expect("prompt message");
            messages = queued;
            messages.push(prompt);
        }

        let turn_id = self
            .journal_turn_started(operation_id.clone(), consumed_queue_ids)
            .await
            .unwrap_or_else(|_| crate::session::durability::new_id("turn"));

        let before_result = match self
            .shared
            .hooks
            .emit_before_agent_start(&BeforeAgentStartEvent {
                prompt: text,
                images: images.clone(),
                system_prompt: turn_state.system_prompt.clone(),
                resources: turn_state.resources.clone(),
            })
            .await
        {
            Ok(r) => r,
            Err(error) => {
                let _ = self
                    .journal_turn_finished(
                        turn_id,
                        operation_id,
                        crate::session::durability::OperationOutcome::Failed,
                    )
                    .await;
                return Err(error);
            }
        };

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
                    let _ = self
                        .journal_turn_finished(
                            turn_id,
                            operation_id,
                            crate::session::durability::OperationOutcome::Failed,
                        )
                        .await;
                    return Err(AgentHarnessError::new(AgentHarnessErrorCode::InvalidState, message));
                }
                Err(error) => {
                    let _ = self
                        .journal_turn_finished(
                            turn_id,
                            operation_id,
                            crate::session::durability::OperationOutcome::Failed,
                        )
                        .await;
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
        let emit: crate::runtime::AgentEventCallback = Arc::new(move |event| {
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
                let outcome = if abort_token.is_cancelled() {
                    crate::session::durability::OperationOutcome::Interrupted
                } else {
                    crate::session::durability::OperationOutcome::Failed
                };
                let _ = self
                    .journal_turn_finished(turn_id, operation_id, outcome)
                    .await;
                return self
                    .emit_run_failure(&model, &error, abort_token.is_cancelled(), &emit)
                    .await;
            }
        };

        if let Err(error) = self.flush_pending_session_writes().await {
            let _ = self
                .journal_turn_finished(
                    turn_id,
                    operation_id,
                    crate::session::durability::OperationOutcome::Failed,
                )
                .await;
            return Err(error);
        }
        let _ = self
            .journal_turn_finished(
                turn_id,
                operation_id,
                crate::session::durability::OperationOutcome::Completed,
            )
            .await;

        for message in run_result.into_iter().rev() {
            if let Some(assistant) = message.as_llm()
                && let Message::Assistant(assistant) = assistant
            {
                if let Some(goal_runtime) = &self.shared.goal_runtime {
                    let mode = *self.shared.collaboration_mode.lock().await;
                    match goal_runtime.finish_turn(mode, Some(&assistant.usage)).await {
                        Ok(GoalTurnFinish::BudgetLimited(goal)) => {
                            let steering = GoalRuntime::budget_steering(&goal);
                            let _ = self
                                .push_durable_queue(
                                    crate::session::durability::QueueKind::NextTurn,
                                    llm_message_to_agent(Message::User {
                                        content: UserContent::Text(steering),
                                        timestamp: now_ms(),
                                    }),
                                )
                                .await;
                        }
                        Ok(GoalTurnFinish::Continuation(goal)) => {
                            let steering = GoalRuntime::continuation_steering(&goal);
                            let _ = self
                                .push_durable_queue(
                                    crate::session::durability::QueueKind::NextTurn,
                                    llm_message_to_agent(Message::User {
                                        content: UserContent::Text(steering),
                                        timestamp: now_ms(),
                                    }),
                                )
                                .await;
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
