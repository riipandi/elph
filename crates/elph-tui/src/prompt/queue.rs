use std::collections::VecDeque;

/// Messages typed while the agent is busy — executed in order after the current turn ends.
#[derive(Debug, Default, Clone)]
pub struct PromptQueue {
    pending: VecDeque<String>,
}

impl PromptQueue {
    pub fn len(&self) -> usize {
        self.pending.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    pub fn push_back(&mut self, text: impl Into<String>) {
        self.pending.push_back(text.into());
    }

    /// High-priority message (steering) — runs before other queued items.
    pub fn push_front(&mut self, text: impl Into<String>) {
        self.pending.push_front(text.into());
    }

    pub fn pop_front(&mut self) -> Option<String> {
        self.pending.pop_front()
    }

    pub fn clear(&mut self) {
        self.pending.clear();
    }
}
