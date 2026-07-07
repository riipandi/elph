//! Steering and follow-up message queues.

use crate::types::{AgentMessage, QueueMode};

#[derive(Clone)]
pub struct PendingMessageQueue {
    messages: std::sync::Arc<std::sync::Mutex<Vec<AgentMessage>>>,
    mode: std::sync::Arc<std::sync::Mutex<QueueMode>>,
}

impl PendingMessageQueue {
    pub fn new(mode: QueueMode) -> Self {
        Self {
            messages: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
            mode: std::sync::Arc::new(std::sync::Mutex::new(mode)),
        }
    }

    pub fn set_mode(&self, mode: QueueMode) {
        *self.mode.lock().expect("queue mode mutex") = mode;
    }

    pub fn mode(&self) -> QueueMode {
        *self.mode.lock().expect("queue mode mutex")
    }

    pub fn enqueue(&self, message: AgentMessage) {
        self.messages.lock().expect("queue mutex").push(message);
    }

    pub fn has_items(&self) -> bool {
        !self.messages.lock().expect("queue mutex").is_empty()
    }

    pub fn drain(&self) -> Vec<AgentMessage> {
        let mut messages = self.messages.lock().expect("queue mutex");
        let mode = *self.mode.lock().expect("queue mode mutex");
        match mode {
            QueueMode::All => {
                let drained = messages.clone();
                messages.clear();
                drained
            }
            QueueMode::OneAtATime => {
                if messages.is_empty() {
                    Vec::new()
                } else {
                    messages.drain(..1).collect()
                }
            }
        }
    }

    pub fn clear(&self) {
        self.messages.lock().expect("queue mutex").clear();
    }
}
