//! Steering and follow-up queue draining.

use crate::session::durability::QueueKind;
use crate::types::{AgentMessage, QueueMode};

use super::super::AgentHarness;

impl<S> AgentHarness<S>
where
    S: crate::session::types::SessionStorage + Clone + Send + Sync + 'static,
    S::Metadata: crate::session::types::HasSessionId + Send + Sync,
{
    pub(super) async fn drain_queued_messages(&self, steering: bool) -> Vec<AgentMessage> {
        if steering {
            self.drain_queue(
                &self.shared.steer_queue,
                *self.shared.steering_queue_mode.lock().await,
                QueueKind::Steer,
            )
            .await
        } else {
            self.drain_queue(
                &self.shared.follow_up_queue,
                *self.shared.follow_up_queue_mode.lock().await,
                QueueKind::FollowUp,
            )
            .await
        }
    }

    async fn drain_queue(
        &self,
        queue: &tokio::sync::Mutex<Vec<(String, AgentMessage)>>,
        mode: QueueMode,
        kind: QueueKind,
    ) -> Vec<AgentMessage> {
        let count = {
            let guard = queue.lock().await;
            if mode == QueueMode::All {
                guard.len()
            } else {
                1.min(guard.len())
            }
        };
        let drained: Vec<(String, AgentMessage)> = queue.lock().await.drain(..count).collect();
        if drained.is_empty() {
            return Vec::new();
        }
        if let Err(_error) = self.emit_queue_update().await {
            let mut guard = queue.lock().await;
            for item in drained.into_iter().rev() {
                guard.insert(0, item);
            }
            return Vec::new();
        }
        let ids: Vec<String> = drained.iter().map(|(id, _)| id.clone()).collect();
        let messages: Vec<AgentMessage> = drained.into_iter().map(|(_, m)| m).collect();
        // Journal consume only after successful dequeue + notify (stable ids match enqueue).
        let _ = self.journal_queue_consume(kind, ids, None).await;
        messages
    }
}
