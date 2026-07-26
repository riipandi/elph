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
        // Transcript card title matches live slash echo (`skill:name [args]`).
        let prompt_title = match additional_instructions.map(str::trim).filter(|s| !s.is_empty()) {
            Some(args) => format!("skill:{name} {args}"),
            None => format!("skill:{name}"),
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
                    AgentHarnessError::new(AgentHarnessErrorCode::InvalidArgument, format!("Unknown skill: {name}"))
                })?;
            let text = format_skill_invocation(skill, additional_instructions);
            self.execute_turn(turn_state, text, None).await
        }
        .await;
        *self.shared.pending_prompt_meta.lock().await = None;
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
        // Transcript card title matches live slash echo (`name [args…]`).
        let prompt_title = if args.is_empty() {
            name.to_string()
        } else {
            format!("{name} {}", args.join(" "))
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
                    AgentHarnessError::new(
                        AgentHarnessErrorCode::InvalidArgument,
                        format!("Unknown prompt template: {name}"),
                    )
                })?;
            let text = format_prompt_template_invocation(template, args);
            self.execute_turn(turn_state, text, None).await
        }
        .await;
        *self.shared.pending_prompt_meta.lock().await = None;
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
    /// Returns the promoted message when a follow-up existed. Rejects while idle without
    /// temporarily removing the message (avoids a lost-item window under concurrent drain).
    pub async fn promote_follow_up_front_to_steer(&self) -> HarnessOpResult<Option<AgentMessage>> {
        if self.phase_async().await == AgentHarnessPhase::Idle {
            return Err(AgentHarnessError::new(
                AgentHarnessErrorCode::InvalidState,
                "Cannot promote follow-up to steer while idle",
            ));
        }
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
        // Re-check after dequeue: turn may have ended while we held the follow-up lock.
        if self.phase_async().await == AgentHarnessPhase::Idle {
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
            self.shared
                .pending_session_writes
                .lock()
                .await
                .push(PendingSessionWrite::Custom {
                    custom_type: custom_type.into(),
                    data,
                });
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
            self.shared
                .pending_session_writes
                .lock()
                .await
                .push(PendingSessionWrite::Message { message });
        }
        Ok(())
    }
}
