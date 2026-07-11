//! Agent harness run loop and turn execution.

mod event_handling;
mod loop_config;
mod queue_drain;
mod session_writes;
mod turn_execution;
mod turn_state;

use tokio::sync::{Mutex, oneshot};
use tokio_util::sync::CancellationToken;

use crate::harness::types::{
    AbortResult, AgentHarnessError, AgentHarnessErrorCode, AgentHarnessOwnEvent, AgentHarnessPhase, QueueUpdateEvent,
};
use crate::types::AgentMessage;

use super::{ActiveRun, AgentHarness, HarnessOpResult};

impl<S> AgentHarness<S>
where
    S: crate::session::types::SessionStorage + Clone + Send + Sync + 'static,
    S::Metadata: crate::session::types::HasSessionId + Send + Sync,
{
    pub async fn abort(&self) -> HarnessOpResult<AbortResult> {
        let cleared_steer: Vec<AgentMessage> = self.shared.steer_queue.lock().await.drain(..).collect();
        let cleared_follow_up: Vec<AgentMessage> = self.shared.follow_up_queue.lock().await.drain(..).collect();
        if let Some(run) = self.shared.active_run.lock().await.as_ref() {
            run.abort_token.cancel();
        }

        let mut errors = Vec::new();
        if let Err(error) = self.emit_queue_update().await {
            errors.push(error.to_string());
        }
        if let Err(error) = self.wait_for_idle().await {
            errors.push(error.to_string());
        }
        if let Err(error) = self
            .emit_own(AgentHarnessOwnEvent::Abort(crate::harness::types::AbortEvent {
                cleared_steer: cleared_steer.clone(),
                cleared_follow_up: cleared_follow_up.clone(),
            }))
            .await
        {
            errors.push(error.to_string());
        }

        if !errors.is_empty() {
            return Err(AgentHarnessError::new(AgentHarnessErrorCode::Hook, errors.join("; ")));
        }

        Ok(AbortResult {
            cleared_steer,
            cleared_follow_up,
        })
    }

    pub async fn wait_for_idle(&self) -> HarnessOpResult<()> {
        // Do not `.await` another mutex while holding `active_run` — that stalls
        // `finish_run` (same lock). `try_lock` is enough: only waiters touch idle_rx.
        let rx = {
            let guard = self.shared.active_run.lock().await;
            match guard.as_ref() {
                Some(run) => run.idle_rx.try_lock().ok().and_then(|mut slot| slot.take()),
                None => None,
            }
        };
        if let Some(rx) = rx {
            let _ = rx.await;
        }
        Ok(())
    }

    pub(in crate::harness::agent_harness) async fn phase_async(&self) -> AgentHarnessPhase {
        *self.shared.phase.lock().await
    }

    pub(in crate::harness::agent_harness) async fn begin_run(&self) {
        let (idle_tx, idle_rx) = oneshot::channel();
        let abort_token = CancellationToken::new();
        *self.shared.active_run.lock().await = Some(ActiveRun {
            idle_tx,
            idle_rx: Mutex::new(Some(idle_rx)),
            abort_token,
        });
    }

    pub(in crate::harness::agent_harness) async fn finish_run(&self) {
        if let Some(run) = self.shared.active_run.lock().await.take() {
            let _ = run.idle_tx.send(());
        }
    }

    pub(in crate::harness::agent_harness) async fn emit_own(&self, event: AgentHarnessOwnEvent) -> HarnessOpResult<()> {
        self.shared
            .hooks
            .emit_subscriber(crate::harness::hooks::AgentHarnessEvent::Own(event), None)
            .await
    }

    pub(in crate::harness::agent_harness) async fn emit_queue_update(&self) -> HarnessOpResult<()> {
        self.emit_own(AgentHarnessOwnEvent::QueueUpdate(QueueUpdateEvent {
            steer: self.shared.steer_queue.lock().await.clone(),
            follow_up: self.shared.follow_up_queue.lock().await.clone(),
            next_turn: self.shared.next_turn_queue.lock().await.clone(),
        }))
        .await
    }
}
