//! Agent harness run loop and turn execution.

mod event_handling;
mod loop_config;
mod queue_drain;
mod session_writes;
mod turn_execution;
mod turn_state;

use tokio::sync::Mutex;
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

use crate::agent::harness::types::AbortResult;
use crate::agent::harness::types::AgentHarnessError;
use crate::agent::harness::types::AgentHarnessErrorCode;
use crate::agent::harness::types::AgentHarnessOwnEvent;
use crate::agent::harness::types::AgentHarnessPhase;
use crate::agent::harness::types::QueueUpdateEvent;
use crate::types::AgentMessage;

use super::{ActiveRun, AgentHarness, HarnessOpResult};

impl<S> AgentHarness<S>
where
    S: crate::session::types::SessionStorage + Clone + Send + Sync + 'static,
    S::Metadata: crate::session::types::HasSessionId + Send + Sync,
{
    pub async fn abort(&self) -> HarnessOpResult<AbortResult> {
        let cleared_steer_items: Vec<(String, AgentMessage)> = self.shared.steer_queue.lock().await.drain(..).collect();
        let cleared_follow_up_items: Vec<(String, AgentMessage)> =
            self.shared.follow_up_queue.lock().await.drain(..).collect();
        let steer_ids: Vec<String> = cleared_steer_items.iter().map(|(id, _)| id.clone()).collect();
        let follow_ids: Vec<String> = cleared_follow_up_items.iter().map(|(id, _)| id.clone()).collect();
        let cleared_steer: Vec<AgentMessage> = cleared_steer_items.into_iter().map(|(_, m)| m).collect();
        let cleared_follow_up: Vec<AgentMessage> = cleared_follow_up_items.into_iter().map(|(_, m)| m).collect();
        let _ = self
            .journal_queue_consume(crate::session::durability::QueueKind::Steer, steer_ids, None)
            .await;
        let _ = self
            .journal_queue_consume(crate::session::durability::QueueKind::FollowUp, follow_ids, None)
            .await;
        let control = self.shared.agent_control.lock().await.clone();
        // Run health check before aborting to clean up stuck agents
        control.health_check_stuck_pending(120).await; // 2 minute timeout for stuck pending
        control.abort_all_running().await;
        self.cancel_active_run().await?;

        let mut errors = Vec::new();
        if let Err(error) = self.emit_queue_update().await {
            errors.push(error.to_string());
        }
        if let Err(error) = self
            .emit_own(AgentHarnessOwnEvent::Abort(crate::agent::harness::types::AbortEvent {
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

    /// Cancel the active turn and wait for the harness to return to idle.
    pub async fn cancel_active_run(&self) -> HarnessOpResult<()> {
        if let Some(run) = self.shared.active_run.lock().await.as_ref() {
            run.abort_token.cancel();
        }
        self.wait_for_idle().await
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

    pub(in crate::agent::harness) async fn phase_async(&self) -> AgentHarnessPhase {
        *self.shared.phase.lock().await
    }

    pub(in crate::agent::harness) async fn begin_run(&self) {
        let (idle_tx, idle_rx) = oneshot::channel();
        let abort_token = CancellationToken::new();
        *self.shared.active_run.lock().await = Some(ActiveRun {
            idle_tx,
            idle_rx: Mutex::new(Some(idle_rx)),
            abort_token,
        });
    }

    pub(in crate::agent::harness) async fn finish_run(&self) {
        if let Some(run) = self.shared.active_run.lock().await.take() {
            let _ = run.idle_tx.send(());
        }
    }

    pub(in crate::agent::harness) async fn emit_own(&self, event: AgentHarnessOwnEvent) -> HarnessOpResult<()> {
        self.shared
            .hooks
            .emit_subscriber(crate::agent::harness::hooks::AgentHarnessEvent::Own(event), None)
            .await
    }

    pub(in crate::agent::harness) async fn emit_queue_update(&self) -> HarnessOpResult<()> {
        let (steer, follow_up, next_turn) = self.queue_messages_snapshot().await;
        self.emit_own(AgentHarnessOwnEvent::QueueUpdate(QueueUpdateEvent {
            steer,
            follow_up,
            next_turn,
        }))
        .await
    }
}
