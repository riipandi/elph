//! Agent loop event handling and session persistence.

use tokio_util::sync::CancellationToken;

use crate::harness::hooks::AgentHarnessEvent;
use crate::harness::types::{AgentHarnessOwnEvent, AgentHarnessPhase};
use crate::types::AgentEvent;

use super::super::helpers::session_error;
use super::super::{AgentHarness, HarnessOpResult};

impl<S> AgentHarness<S>
where
    S: crate::session::types::SessionStorage + Clone + Send + Sync + 'static,
    S::Metadata: crate::session::types::HasSessionId + Send + Sync,
{
    pub(super) async fn handle_agent_event(
        &self,
        event: AgentEvent,
        signal: Option<CancellationToken>,
    ) -> HarnessOpResult<()> {
        match &event {
            AgentEvent::MessageEnd { message } => {
                self.shared
                    .session
                    .lock()
                    .await
                    .append_message(message.clone())
                    .await
                    .map_err(session_error)?;
                self.shared
                    .hooks
                    .emit_subscriber(AgentHarnessEvent::Agent(event.clone()), signal)
                    .await?;
                return Ok(());
            }
            AgentEvent::TurnEnd { message, .. } => {
                self.maybe_request_plan_confirmation(message).await?;
                let event_error = self
                    .shared
                    .hooks
                    .emit_subscriber(AgentHarnessEvent::Agent(event.clone()), signal.clone())
                    .await
                    .err();
                let had_pending = !self.shared.pending_session_writes.lock().await.is_empty();
                self.flush_pending_session_writes().await?;
                if let Some(error) = event_error {
                    return Err(error);
                }
                self.emit_own(AgentHarnessOwnEvent::SavePoint(crate::harness::types::SavePointEvent {
                    had_pending_mutations: had_pending,
                }))
                .await?;
                return Ok(());
            }
            AgentEvent::AgentEnd { .. } => {
                self.flush_pending_session_writes().await?;
                *self.shared.phase.lock().await = AgentHarnessPhase::Idle;
                self.shared
                    .hooks
                    .emit_subscriber(AgentHarnessEvent::Agent(event.clone()), signal.clone())
                    .await?;
                let next_turn_count = self.shared.next_turn_queue.lock().await.len();
                self.emit_own(AgentHarnessOwnEvent::Settled(crate::harness::types::SettledEvent {
                    next_turn_count,
                }))
                .await?;
                return Ok(());
            }
            _ => {}
        }
        self.shared
            .hooks
            .emit_subscriber(AgentHarnessEvent::Agent(event.clone()), signal)
            .await
    }
}
