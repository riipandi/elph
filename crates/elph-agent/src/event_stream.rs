//! Agent event stream — mirrors pi-ai `EventStream` for `AgentEvent`.

use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use crate::types::{AgentEvent, AgentMessage};

#[derive(Clone)]
pub struct AgentEventStream {
    queue: Arc<Mutex<EventQueue>>,
}

struct EventQueue {
    events: Vec<AgentEvent>,
    read_index: usize,
    done: bool,
    final_messages: Option<Vec<AgentMessage>>,
    waiters: Vec<tokio::sync::oneshot::Sender<()>>,
}

impl Default for AgentEventStream {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentEventStream {
    pub fn new() -> Self {
        Self {
            queue: Arc::new(Mutex::new(EventQueue {
                events: Vec::new(),
                read_index: 0,
                done: false,
                final_messages: None,
                waiters: Vec::new(),
            })),
        }
    }

    pub fn push(&self, event: AgentEvent) {
        let mut q = self.queue.lock().expect("agent event stream mutex poisoned");
        if q.done {
            return;
        }
        if let AgentEvent::AgentEnd { messages } = &event {
            q.final_messages = Some(messages.clone());
            q.done = true;
        }
        q.events.push(event);
        let waiters = std::mem::take(&mut q.waiters);
        drop(q);
        for waiter in waiters {
            let _ = waiter.send(());
        }
    }

    pub fn end(&self, messages: Vec<AgentMessage>) {
        self.push(AgentEvent::AgentEnd { messages });
    }

    pub async fn next_event(&mut self) -> Option<AgentEvent> {
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

    pub async fn result(&mut self) -> Vec<AgentMessage> {
        while let Some(event) = self.next_event().await {
            if let AgentEvent::AgentEnd { messages } = event {
                return messages;
            }
        }
        self.queue
            .lock()
            .expect("agent event stream mutex poisoned")
            .final_messages
            .clone()
            .unwrap_or_default()
    }

    fn pop_next(&self) -> Option<AgentEvent> {
        let mut q = self.queue.lock().expect("agent event stream mutex poisoned");
        if q.read_index < q.events.len() {
            let event = q.events[q.read_index].clone();
            q.read_index += 1;
            return Some(event);
        }
        None
    }

    fn is_done_sync(&self) -> bool {
        let q = self.queue.lock().expect("agent event stream mutex poisoned");
        q.done && q.read_index >= q.events.len()
    }

    fn register_waiter(&self) -> tokio::sync::oneshot::Receiver<()> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let mut q = self.queue.lock().expect("agent event stream mutex poisoned");
        if q.read_index < q.events.len() || q.done {
            let _ = tx.send(());
        } else {
            q.waiters.push(tx);
        }
        rx
    }
}

pub type AgentEventSink = Arc<dyn Fn(AgentEvent) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>;
