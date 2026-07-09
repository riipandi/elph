//! Steering and follow-up message queues.

use std::sync::Arc;

use parking_lot::Mutex;

use crate::types::{AgentMessage, QueueMode};

#[derive(Clone)]
pub struct PendingMessageQueue {
    messages: Arc<Mutex<Vec<AgentMessage>>>,
    mode: Arc<Mutex<QueueMode>>,
}

impl PendingMessageQueue {
    pub fn new(mode: QueueMode) -> Self {
        Self {
            messages: Arc::new(Mutex::new(Vec::new())),
            mode: Arc::new(Mutex::new(mode)),
        }
    }

    pub fn set_mode(&self, mode: QueueMode) {
        *self.mode.lock() = mode;
    }

    pub fn mode(&self) -> QueueMode {
        *self.mode.lock()
    }

    pub fn enqueue(&self, message: AgentMessage) {
        self.messages.lock().push(message);
    }

    pub fn has_items(&self) -> bool {
        !self.messages.lock().is_empty()
    }

    pub fn drain(&self) -> Vec<AgentMessage> {
        let mode = *self.mode.lock();
        let mut messages = self.messages.lock();
        match mode {
            QueueMode::All => std::mem::take(&mut *messages),
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
        self.messages.lock().clear();
    }
}
