//! Steering and follow-up queue draining.

use crate::types::{AgentMessage, QueueMode};

use super::super::AgentHarness;

impl<S> AgentHarness<S>
where
    S: crate::session::types::SessionStorage + Clone + Send + Sync + 'static,
    S::Metadata: crate::session::types::HasSessionId + Send + Sync,
{
    pub(super) async fn drain_queued_messages(&self, steering: bool) -> Vec<AgentMessage> {
        if steering {
            self.drain_queue(&self.shared.steer_queue, *self.shared.steering_queue_mode.lock().await, true)
                .await
        } else {
            self.drain_queue(
                &self.shared.follow_up_queue,
                *self.shared.follow_up_queue_mode.lock().await,
                false,
            )
            .await
        }
    }

    async fn drain_queue(
        &self,
        queue: &tokio::sync::Mutex<Vec<AgentMessage>>,
        mode: QueueMode,
        is_steer: bool,
    ) -> Vec<AgentMessage> {
        let count = {
            let guard = queue.lock().await;
            if mode == QueueMode::All {
                guard.len()
            } else {
                1.min(guard.len())
            }
        };
        let messages: Vec<_> = queue.lock().await.drain(..count).collect();
        if messages.is_empty() {
            return messages;
        }
        if let Err(error) = self.emit_queue_update().await {
            let mut guard = queue.lock().await;
            for message in messages.into_iter().rev() {
                guard.insert(0, message);
            }
            let _ = (error, is_steer);
            return Vec::new();
        }
        messages
    }
}
