use std::sync::{Arc, Mutex};

use crate::types::{AssistantMessage, AssistantMessageEvent};

/// Async event stream for assistant message streaming.
#[derive(Clone)]
pub struct AssistantMessageEventStream {
    queue: Arc<Mutex<EventQueue>>,
}

/// Compact consumed prefix once this many events have been read.
const EVENT_COMPACT_THRESHOLD: usize = 64;

struct EventQueue {
    events: Vec<AssistantMessageEvent>,
    read_index: usize,
    done: bool,
    final_result: Option<AssistantMessage>,
    waiters: Vec<tokio::sync::oneshot::Sender<()>>,
}

fn compact_consumed_events(queue: &mut EventQueue) {
    if queue.read_index >= EVENT_COMPACT_THRESHOLD {
        queue.events.drain(0..queue.read_index);
        queue.read_index = 0;
    }
}

impl Default for AssistantMessageEventStream {
    fn default() -> Self {
        Self::new()
    }
}

impl AssistantMessageEventStream {
    pub fn new() -> Self {
        Self {
            queue: Arc::new(Mutex::new(EventQueue {
                events: Vec::new(),
                read_index: 0,
                done: false,
                final_result: None,
                waiters: Vec::new(),
            })),
        }
    }

    pub fn clone_handle(&self) -> Self {
        self.clone()
    }

    pub fn failed(message: impl Into<String>) -> Self {
        let stream = Self::new();
        let mut partial = AssistantMessage::empty(&crate::types::Model {
            id: String::new(),
            name: String::new(),
            api: String::new(),
            provider: String::new(),
            base_url: String::new(),
            reasoning: false,
            thinking_level_map: None,
            input: vec![],
            cost: crate::types::ModelCost {
                input: 0.0,
                output: 0.0,
                cache_read: 0.0,
                cache_write: 0.0,
            },
            context_window: 0,
            max_tokens: 0,
            headers: None,
            openai_completions_compat: None,
            openai_responses_compat: None,
            anthropic_compat: None,
        });
        partial.stop_reason = crate::types::StopReason::Error;
        partial.error_message = Some(message.into());
        stream.push(AssistantMessageEvent::Error {
            reason: crate::types::StopReason::Error,
            error: partial,
        });
        stream.end();
        stream
    }

    pub async fn next_event(&mut self) -> Option<AssistantMessageEvent> {
        loop {
            if let Some(event) = self.pop_next() {
                return Some(event);
            }
            if self.is_done_sync() {
                return None;
            }
            let rx = self.register_waiter();
            let _ = rx.await;
        }
    }

    pub fn is_done(&self) -> bool {
        self.is_done_sync()
    }

    /// Push an event in-order. Must be synchronous to preserve stream ordering.
    pub fn push(&self, event: AssistantMessageEvent) {
        let mut q = self.queue.lock().expect("event stream mutex poisoned");
        if q.done {
            return;
        }

        match &event {
            AssistantMessageEvent::Done { message, .. } => {
                q.final_result = Some(message.clone());
                q.done = true;
            }
            AssistantMessageEvent::Error { error, .. } => {
                q.final_result = Some(error.clone());
                q.done = true;
            }
            _ => {}
        }

        q.events.push(event);
        let waiters = std::mem::take(&mut q.waiters);
        for waiter in waiters {
            let _ = waiter.send(());
        }
    }

    pub fn end(&self) {
        let mut q = self.queue.lock().expect("event stream mutex poisoned");
        if q.done {
            return;
        }
        q.done = true;
        let waiters = std::mem::take(&mut q.waiters);
        for waiter in waiters {
            let _ = waiter.send(());
        }
    }

    pub async fn result(&self) -> AssistantMessage {
        loop {
            if let Some(result) = self.final_result_sync() {
                return result;
            }
            if self.is_done_sync() {
                break;
            }
            let rx = self.register_waiter();
            let _ = rx.await;
        }
        self.final_result_sync().unwrap_or_else(|| {
            AssistantMessage::empty(&crate::types::Model {
                id: String::new(),
                name: String::new(),
                api: String::new(),
                provider: String::new(),
                base_url: String::new(),
                reasoning: false,
                thinking_level_map: None,
                input: vec![],
                cost: crate::types::ModelCost {
                    input: 0.0,
                    output: 0.0,
                    cache_read: 0.0,
                    cache_write: 0.0,
                },
                context_window: 0,
                max_tokens: 0,
                headers: None,
                openai_completions_compat: None,
                openai_responses_compat: None,
                anthropic_compat: None,
            })
        })
    }

    fn pop_next(&self) -> Option<AssistantMessageEvent> {
        let mut q = self.queue.lock().expect("event stream mutex poisoned");
        if q.read_index < q.events.len() {
            let event = q.events[q.read_index].clone();
            q.read_index += 1;
            compact_consumed_events(&mut q);
            Some(event)
        } else {
            None
        }
    }

    fn is_done_sync(&self) -> bool {
        self.queue.lock().expect("event stream mutex poisoned").done
    }

    fn final_result_sync(&self) -> Option<AssistantMessage> {
        self.queue
            .lock()
            .expect("event stream mutex poisoned")
            .final_result
            .clone()
    }

    fn register_waiter(&self) -> tokio::sync::oneshot::Receiver<()> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let mut q = self.queue.lock().expect("event stream mutex poisoned");
        if q.read_index < q.events.len() || q.done {
            let _ = tx.send(());
        } else {
            q.waiters.push(tx);
        }
        rx
    }
}

pub struct EventStreamIterator {
    queue: Arc<Mutex<EventQueue>>,
    index: usize,
}

impl AssistantMessageEventStream {
    pub fn into_stream(self) -> EventStreamIterator {
        EventStreamIterator {
            queue: self.queue,
            index: 0,
        }
    }
}

impl EventStreamIterator {
    pub async fn next(&mut self) -> Option<AssistantMessageEvent> {
        loop {
            let next = {
                let mut q = self.queue.lock().expect("event stream mutex poisoned");
                if self.index < q.events.len() {
                    let event = q.events[self.index].clone();
                    self.index += 1;
                    if self.index >= EVENT_COMPACT_THRESHOLD {
                        q.events.drain(0..self.index);
                        self.index = 0;
                    }
                    Some(event)
                } else {
                    None
                }
            };
            if next.is_some() {
                return next;
            }
            let (done, register_waiter) = {
                let q = self.queue.lock().expect("event stream mutex poisoned");
                let done = q.done;
                let register_waiter = self.index >= q.events.len() && !q.done;
                (done, register_waiter)
            };
            if done {
                return None;
            }
            if !register_waiter {
                continue;
            }
            let rx = {
                let (tx, rx) = tokio::sync::oneshot::channel();
                let mut q = self.queue.lock().expect("event stream mutex poisoned");
                if self.index < q.events.len() || q.done {
                    let _ = tx.send(());
                } else {
                    q.waiters.push(tx);
                }
                rx
            };
            let _ = rx.await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{AssistantMessageEvent, Model, StopReason};

    fn test_model() -> Model {
        Model {
            id: "test".to_string(),
            name: "test".to_string(),
            api: "test".to_string(),
            provider: "test".to_string(),
            base_url: "http://localhost".to_string(),
            reasoning: false,
            thinking_level_map: None,
            input: vec!["text".to_string()],
            cost: crate::types::ModelCost {
                input: 0.0,
                output: 0.0,
                cache_read: 0.0,
                cache_write: 0.0,
            },
            context_window: 128_000,
            max_tokens: 16_384,
            headers: None,
            openai_completions_compat: None,
            openai_responses_compat: None,
            anthropic_compat: None,
        }
    }

    #[tokio::test]
    async fn consumed_events_are_compacted_to_bound_memory() {
        let stream = AssistantMessageEventStream::new();
        let model = test_model();
        for _ in 0..EVENT_COMPACT_THRESHOLD + 8 {
            stream.push(AssistantMessageEvent::TextDelta {
                content_index: 0,
                delta: "x".to_string(),
                partial: AssistantMessage::empty(&model),
            });
        }
        stream.end();

        let queue = stream.queue.clone();
        let mut events = stream.into_stream();
        let mut consumed = 0usize;
        while events.next().await.is_some() {
            consumed += 1;
        }

        let retained = queue.lock().expect("event stream mutex poisoned").events.len();
        assert_eq!(consumed, EVENT_COMPACT_THRESHOLD + 8);
        assert!(retained < EVENT_COMPACT_THRESHOLD);
    }

    #[tokio::test]
    async fn waiter_is_not_registered_when_events_are_already_available() {
        let mut stream = AssistantMessageEventStream::new();
        let model = test_model();
        let mut partial = AssistantMessage::empty(&model);
        partial.stop_reason = StopReason::Stop;
        stream.push(AssistantMessageEvent::Done {
            reason: StopReason::Stop,
            message: partial,
        });
        stream.end();

        let event = stream.next_event().await.expect("stream event");
        assert!(matches!(event, AssistantMessageEvent::Done { .. }));
    }
}
