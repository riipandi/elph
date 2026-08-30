//! Agent harness prompt and queue operations.

use elph_ai::AssistantMessage;

use crate::agent::harness::types::{AgentHarnessError, AgentHarnessPromptOptions, QueueUpdateEvent};
use crate::agent::harness::types::{AgentHarnessErrorCode, AgentHarnessPhase, PendingSessionWrite};
use crate::prompt::format_prompt_template_invocation;
use crate::session::types::{HasSessionId, SessionStorage};
use crate::skills::format_skill_invocation;
use crate::types::AgentMessage;

use super::helpers::{create_user_message, session_error};
use super::{AgentHarness, HarnessOpResult};

impl<S> AgentHarness<S>
where
    S: SessionStorage + Clone + Send + Sync + 'static,
    S::Metadata: HasSessionId + Send + Sync,
{
    #[cfg_attr(feature = "tracing", fastrace::trace(name = "elph.agent.turn"))]
    pub async fn prompt(
        &self,
        text: impl Into<String>,
        options: Option<AgentHarnessPromptOptions>,
    ) -> HarnessOpResult<AssistantMessage> {
        if self.phase_async().await != AgentHarnessPhase::Idle {
            log::warn!("harness prompt rejected: busy");
            return Err(AgentHarnessError::new(AgentHarnessErrorCode::Busy, "AgentHarness is busy"));
        }
        log::debug!("harness turn start");
        crate::trace::add_event("turn_start");
        *self.shared.phase.lock().await = AgentHarnessPhase::Turn;
        self.begin_run().await;
        let mut cleanup = self.run_cleanup_guard();
        let op_id = self
            .journal_operation_started(crate::session::durability::OperationKind::Run)
            .await
            .unwrap_or_else(|_| crate::session::durability::new_id("op"));
        let result = async {
            let turn_state = self.create_turn_state().await?;
            self.execute_turn(turn_state, text.into(), options, op_id.clone()).await
        }
        .await;
        let outcome = if result.is_ok() {
            crate::session::durability::OperationOutcome::Completed
        } else {
            crate::session::durability::OperationOutcome::Failed
        };
        let _ = self
            .journal_operation_finished(op_id, outcome, result.as_ref().err().map(|e| e.to_string()))
            .await;
        if result.is_err() {
            log::warn!(
                "harness turn failed: {}",
                result.as_ref().err().map(|e| e.to_string()).unwrap_or_default()
            );
            *self.shared.phase.lock().await = AgentHarnessPhase::Idle;
        } else {
            log::debug!("harness turn ok");
        }
        self.finish_run().await;
        cleanup.disarm();
        result
    }

    #[cfg_attr(feature = "tracing", fastrace::trace(name = "elph.agent.skill"))]
    pub async fn skill(&self, name: &str, additional_instructions: Option<&str>) -> HarnessOpResult<AssistantMessage> {
        crate::trace::add_property("skill.name", name);
        if self.phase_async().await != AgentHarnessPhase::Idle {
            log::warn!("harness skill rejected: busy name={name}");
            return Err(AgentHarnessError::new(AgentHarnessErrorCode::Busy, "AgentHarness is busy"));
        }
        log::debug!("harness skill start name={name}");
        *self.shared.phase.lock().await = AgentHarnessPhase::Turn;
        self.begin_run().await;
        let mut cleanup = self.run_cleanup_guard();
        let op_id = self
            .journal_operation_started(crate::session::durability::OperationKind::Run)
            .await
            .unwrap_or_else(|_| crate::session::durability::new_id("op"));
        // Transcript + prompt history use `/skill:name [args]` (leading slash required).
        let prompt_title = match additional_instructions.map(str::trim).filter(|s| !s.is_empty()) {
            Some(args) => format!("/skill:{name} {args}"),
            None => format!("/skill:{name}"),
        };
        *self.shared.pending_prompt_meta.lock().await = Some(("skill".into(), prompt_title));
        let result = async {
            let turn_state = self.create_turn_state().await?;
            let skill = turn_state
                .resources
                .skills
                .iter()
                .find(|skill| skill.name == name)
                .ok_or_else(|| {
                    log::warn!("harness skill unknown name={name}");
                    AgentHarnessError::new(AgentHarnessErrorCode::InvalidArgument, format!("Unknown skill: {name}"))
                })?;
            let text = format_skill_invocation(skill, additional_instructions);
            self.execute_turn(turn_state, text, None, op_id.clone()).await
        }
        .await;
        *self.shared.pending_prompt_meta.lock().await = None;
        let outcome = if result.is_ok() {
            crate::session::durability::OperationOutcome::Completed
        } else {
            crate::session::durability::OperationOutcome::Failed
        };
        let _ = self
            .journal_operation_finished(op_id, outcome, result.as_ref().err().map(|e| e.to_string()))
            .await;
        if result.is_err() {
            *self.shared.phase.lock().await = AgentHarnessPhase::Idle;
        }
        self.finish_run().await;
        cleanup.disarm();
        result
    }

    #[cfg_attr(feature = "tracing", fastrace::trace(name = "elph.agent.prompt_template"))]
    pub async fn prompt_from_template(&self, name: &str, args: &[String]) -> HarnessOpResult<AssistantMessage> {
        crate::trace::add_property("template.name", name);
        if self.phase_async().await != AgentHarnessPhase::Idle {
            log::warn!("harness template rejected: busy name={name}");
            return Err(AgentHarnessError::new(AgentHarnessErrorCode::Busy, "AgentHarness is busy"));
        }
        log::debug!("harness template start name={name}");
        *self.shared.phase.lock().await = AgentHarnessPhase::Turn;
        self.begin_run().await;
        let mut cleanup = self.run_cleanup_guard();
        let op_id = self
            .journal_operation_started(crate::session::durability::OperationKind::Run)
            .await
            .unwrap_or_else(|_| crate::session::durability::new_id("op"));
        // Transcript + prompt history keep the leading `/` (`/name [args…]`).
        let prompt_title = if args.is_empty() {
            format!("/{name}")
        } else {
            format!("/{name} {}", args.join(" "))
        };
        *self.shared.pending_prompt_meta.lock().await = Some(("template".into(), prompt_title));
        let result = async {
            let turn_state = self.create_turn_state().await?;
            let template = turn_state
                .resources
                .prompt_templates
                .iter()
                .find(|template| template.name == name)
                .ok_or_else(|| {
                    log::warn!("harness template unknown name={name}");
                    AgentHarnessError::new(
                        AgentHarnessErrorCode::InvalidArgument,
                        format!("Unknown prompt template: {name}"),
                    )
                })?;
            let text = format_prompt_template_invocation(template, args);
            self.execute_turn(turn_state, text, None, op_id.clone()).await
        }
        .await;
        *self.shared.pending_prompt_meta.lock().await = None;
        let outcome = if result.is_ok() {
            crate::session::durability::OperationOutcome::Completed
        } else {
            crate::session::durability::OperationOutcome::Failed
        };
        let _ = self
            .journal_operation_finished(op_id, outcome, result.as_ref().err().map(|e| e.to_string()))
            .await;
        if result.is_err() {
            *self.shared.phase.lock().await = AgentHarnessPhase::Idle;
        }
        self.finish_run().await;
        cleanup.disarm();
        result
    }

    pub async fn steer(
        &self,
        text: impl Into<String>,
        options: Option<AgentHarnessPromptOptions>,
    ) -> HarnessOpResult<()> {
        if self.phase_async().await == AgentHarnessPhase::Idle {
            return Err(AgentHarnessError::new(
                AgentHarnessErrorCode::InvalidState,
                "Cannot steer while idle",
            ));
        }
        let message = create_user_message(text.into(), options.and_then(|o| o.images));
        self.push_durable_queue(crate::session::durability::QueueKind::Steer, message)
            .await?;
        self.emit_queue_update().await
    }

    pub async fn follow_up(
        &self,
        text: impl Into<String>,
        options: Option<AgentHarnessPromptOptions>,
    ) -> HarnessOpResult<()> {
        if self.phase_async().await == AgentHarnessPhase::Idle {
            return Err(AgentHarnessError::new(
                AgentHarnessErrorCode::InvalidState,
                "Cannot follow up while idle",
            ));
        }
        let message = create_user_message(text.into(), options.and_then(|o| o.images));
        self.push_durable_queue(crate::session::durability::QueueKind::FollowUp, message)
            .await?;
        self.emit_queue_update().await
    }

    pub async fn next_turn(
        &self,
        text: impl Into<String>,
        options: Option<AgentHarnessPromptOptions>,
    ) -> HarnessOpResult<()> {
        let message = create_user_message(text.into(), options.and_then(|o| o.images));
        self.push_durable_queue(crate::session::durability::QueueKind::NextTurn, message)
            .await?;
        self.emit_queue_update().await
    }

    /// Snapshot of steer / follow-up / next-turn queues (read-only).
    pub async fn peek_queues(&self) -> QueueUpdateEvent {
        let (steer, follow_up, next_turn) = self.queue_messages_snapshot().await;
        QueueUpdateEvent {
            steer,
            follow_up,
            next_turn,
        }
    }

    /// Remove a steer-queue item by index. Emits [`QueueUpdate`] on success.
    pub async fn remove_steer_at(&self, index: usize) -> HarnessOpResult<Option<AgentMessage>> {
        let removed = {
            let mut guard = self.shared.steer_queue.lock().await;
            if index >= guard.len() {
                None
            } else {
                Some(guard.remove(index))
            }
        };
        if let Some((id, message)) = removed {
            let _ = self
                .journal_queue_consume(crate::session::durability::QueueKind::Steer, vec![id], None)
                .await;
            self.emit_queue_update().await?;
            Ok(Some(message))
        } else {
            Ok(None)
        }
    }

    /// Remove a follow-up queue item by index. Emits [`QueueUpdate`] on success.
    pub async fn remove_follow_up_at(&self, index: usize) -> HarnessOpResult<Option<AgentMessage>> {
        let removed = {
            let mut guard = self.shared.follow_up_queue.lock().await;
            if index >= guard.len() {
                None
            } else {
                Some(guard.remove(index))
            }
        };
        if let Some((id, message)) = removed {
            let _ = self
                .journal_queue_consume(crate::session::durability::QueueKind::FollowUp, vec![id], None)
                .await;
            self.emit_queue_update().await?;
            Ok(Some(message))
        } else {
            Ok(None)
        }
    }

    /// Move the front follow-up message onto the steer queue (interject one queued prompt).
    ///
    /// Returns the promoted message when a follow-up existed. Rejects while idle without
    /// temporarily removing the message (avoids a lost-item window under concurrent drain).
    pub async fn promote_follow_up_front_to_steer(&self) -> HarnessOpResult<Option<AgentMessage>> {
        if self.phase_async().await == AgentHarnessPhase::Idle {
            return Err(AgentHarnessError::new(
                AgentHarnessErrorCode::InvalidState,
                "Cannot promote follow-up to steer while idle",
            ));
        }
        let item = {
            let mut follow = self.shared.follow_up_queue.lock().await;
            if follow.is_empty() {
                None
            } else {
                Some(follow.remove(0))
            }
        };
        let Some((follow_id, message)) = item else {
            return Ok(None);
        };
        // Re-check after dequeue: turn may have ended while we held the follow-up lock.
        if self.phase_async().await == AgentHarnessPhase::Idle {
            self.shared.follow_up_queue.lock().await.insert(0, (follow_id, message));
            return Err(AgentHarnessError::new(
                AgentHarnessErrorCode::InvalidState,
                "Cannot promote follow-up to steer while idle",
            ));
        }
        let _ = self
            .journal_queue_consume(crate::session::durability::QueueKind::FollowUp, vec![follow_id], None)
            .await;
        self.push_durable_queue(crate::session::durability::QueueKind::Steer, message.clone())
            .await?;
        self.emit_queue_update().await?;
        Ok(Some(message))
    }

    /// Clear steer and follow-up queues (keeps next-turn). Emits [`QueueUpdate`].
    pub async fn clear_prompt_queues(&self) -> HarnessOpResult<()> {
        let steer_ids: Vec<String> = self
            .shared
            .steer_queue
            .lock()
            .await
            .drain(..)
            .map(|(id, _)| id)
            .collect();
        let follow_ids: Vec<String> = self
            .shared
            .follow_up_queue
            .lock()
            .await
            .drain(..)
            .map(|(id, _)| id)
            .collect();
        let _ = self
            .journal_queue_consume(crate::session::durability::QueueKind::Steer, steer_ids, None)
            .await;
        let _ = self
            .journal_queue_consume(crate::session::durability::QueueKind::FollowUp, follow_ids, None)
            .await;
        self.emit_queue_update().await
    }

    /// Append a custom metadata entry to the session tree.
    ///
    /// Stored directly when the harness is idle; queued as a pending write otherwise.
    pub async fn append_custom_entry(
        &self,
        custom_type: impl Into<String>,
        data: Option<serde_json::Value>,
    ) -> HarnessOpResult<()> {
        if self.phase_async().await == AgentHarnessPhase::Idle {
            self.shared
                .session
                .lock()
                .await
                .append_custom_entry(&custom_type.into(), data)
                .await
                .map_err(session_error)?;
        } else {
            self.enqueue_pending_write(PendingSessionWrite::Custom {
                custom_type: custom_type.into(),
                data,
            })
            .await?;
        }
        Ok(())
    }

    /// Persist a session display title (`session_info` tree entry).
    ///
    /// Stored directly when idle; queued as a pending write while a turn is running.
    pub async fn set_session_name(&self, name: impl Into<String>) -> HarnessOpResult<()> {
        let name = name.into();
        if self.phase_async().await == AgentHarnessPhase::Idle {
            self.shared
                .session
                .lock()
                .await
                .append_session_name(name)
                .await
                .map_err(session_error)?;
        } else {
            self.enqueue_pending_write(PendingSessionWrite::SessionInfo { name: Some(name) })
                .await?;
        }
        Ok(())
    }

    pub async fn append_message(&self, message: AgentMessage) -> HarnessOpResult<()> {
        let prompt_meta = self.shared.pending_prompt_meta.lock().await.take();
        if self.phase_async().await == AgentHarnessPhase::Idle {
            if let Some((kind, title)) = prompt_meta {
                self.shared
                    .session
                    .lock()
                    .await
                    .append_message_with_prompt(message, title, kind)
                    .await
                    .map_err(session_error)?;
            } else {
                self.shared
                    .session
                    .lock()
                    .await
                    .append_message(message)
                    .await
                    .map_err(session_error)?;
            }
        } else {
            self.enqueue_pending_write(PendingSessionWrite::Message { message })
                .await?;
        }
        Ok(())
    }

    /// Bump the session row's `updated_at` to now without appending a tree entry.
    ///
    /// Keeps resume ordering and retention budgets truthful when a bound turn
    /// performs no writes of its own.
    pub async fn touch_session_timestamp(&self) -> HarnessOpResult<()> {
        self.shared
            .session
            .lock()
            .await
            .touch_timestamp()
            .await
            .map_err(session_error)
    }
}
