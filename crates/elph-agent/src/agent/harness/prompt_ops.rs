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
            return Err(AgentHarnessError::new(AgentHarnessErrorCode::Busy, "AgentHarness is busy"));
        }
        *self.shared.phase.lock().await = AgentHarnessPhase::Turn;
        self.begin_run().await;
        let result = async {
            let turn_state = self.create_turn_state().await?;
            self.execute_turn(turn_state, text.into(), options).await
        }
        .await;
        if result.is_err() {
            *self.shared.phase.lock().await = AgentHarnessPhase::Idle;
        }
        self.finish_run().await;
        result
    }

    pub async fn skill(&self, name: &str, additional_instructions: Option<&str>) -> HarnessOpResult<AssistantMessage> {
        if self.phase_async().await != AgentHarnessPhase::Idle {
            return Err(AgentHarnessError::new(AgentHarnessErrorCode::Busy, "AgentHarness is busy"));
        }
        *self.shared.phase.lock().await = AgentHarnessPhase::Turn;
        self.begin_run().await;
        let result = async {
            let turn_state = self.create_turn_state().await?;
            let skill = turn_state
                .resources
                .skills
                .iter()
                .find(|skill| skill.name == name)
                .ok_or_else(|| {
                    AgentHarnessError::new(AgentHarnessErrorCode::InvalidArgument, format!("Unknown skill: {name}"))
                })?;
            let text = format_skill_invocation(skill, additional_instructions);
            self.execute_turn(turn_state, text, None).await
        }
        .await;
        if result.is_err() {
            *self.shared.phase.lock().await = AgentHarnessPhase::Idle;
        }
        self.finish_run().await;
        result
    }

    pub async fn prompt_from_template(&self, name: &str, args: &[String]) -> HarnessOpResult<AssistantMessage> {
        if self.phase_async().await != AgentHarnessPhase::Idle {
            return Err(AgentHarnessError::new(AgentHarnessErrorCode::Busy, "AgentHarness is busy"));
        }
        *self.shared.phase.lock().await = AgentHarnessPhase::Turn;
        self.begin_run().await;
        let result = async {
            let turn_state = self.create_turn_state().await?;
            let template = turn_state
                .resources
                .prompt_templates
                .iter()
                .find(|template| template.name == name)
                .ok_or_else(|| {
                    AgentHarnessError::new(
                        AgentHarnessErrorCode::InvalidArgument,
                        format!("Unknown prompt template: {name}"),
                    )
                })?;
            let text = format_prompt_template_invocation(template, args);
            self.execute_turn(turn_state, text, None).await
        }
        .await;
        if result.is_err() {
            *self.shared.phase.lock().await = AgentHarnessPhase::Idle;
        }
        self.finish_run().await;
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
        self.shared
            .steer_queue
            .lock()
            .await
            .push(create_user_message(text.into(), options.and_then(|o| o.images)));
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
        self.shared
            .follow_up_queue
            .lock()
            .await
            .push(create_user_message(text.into(), options.and_then(|o| o.images)));
        self.emit_queue_update().await
    }

    pub async fn next_turn(
        &self,
        text: impl Into<String>,
        options: Option<AgentHarnessPromptOptions>,
    ) -> HarnessOpResult<()> {
        self.shared
            .next_turn_queue
            .lock()
            .await
            .push(create_user_message(text.into(), options.and_then(|o| o.images)));
        self.emit_queue_update().await
    }

    /// Snapshot of steer / follow-up / next-turn queues (read-only).
    pub async fn peek_queues(&self) -> QueueUpdateEvent {
        QueueUpdateEvent {
            steer: self.shared.steer_queue.lock().await.clone(),
            follow_up: self.shared.follow_up_queue.lock().await.clone(),
            next_turn: self.shared.next_turn_queue.lock().await.clone(),
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
        if removed.is_some() {
            self.emit_queue_update().await?;
        }
        Ok(removed)
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
        if removed.is_some() {
            self.emit_queue_update().await?;
        }
        Ok(removed)
    }

    /// Move the front follow-up message onto the steer queue (interject one queued prompt).
    ///
    /// Returns the promoted message text when a follow-up existed.
    pub async fn promote_follow_up_front_to_steer(&self) -> HarnessOpResult<Option<AgentMessage>> {
        let message = {
            let mut follow = self.shared.follow_up_queue.lock().await;
            if follow.is_empty() {
                None
            } else {
                Some(follow.remove(0))
            }
        };
        let Some(message) = message else {
            return Ok(None);
        };
        if self.phase_async().await == AgentHarnessPhase::Idle {
            // Cannot steer while idle — put the message back.
            self.shared.follow_up_queue.lock().await.insert(0, message);
            return Err(AgentHarnessError::new(
                AgentHarnessErrorCode::InvalidState,
                "Cannot promote follow-up to steer while idle",
            ));
        }
        self.shared.steer_queue.lock().await.push(message.clone());
        self.emit_queue_update().await?;
        Ok(Some(message))
    }

    /// Clear steer and follow-up queues (keeps next-turn). Emits [`QueueUpdate`].
    pub async fn clear_prompt_queues(&self) -> HarnessOpResult<()> {
        self.shared.steer_queue.lock().await.clear();
        self.shared.follow_up_queue.lock().await.clear();
        self.emit_queue_update().await
    }

    pub async fn append_message(&self, message: AgentMessage) -> HarnessOpResult<()> {
        if self.phase_async().await == AgentHarnessPhase::Idle {
            self.shared
                .session
                .lock()
                .await
                .append_message(message)
                .await
                .map_err(session_error)?;
        } else {
            self.shared
                .pending_session_writes
                .lock()
                .await
                .push(PendingSessionWrite::Message { message });
        }
        Ok(())
    }
}
