//! Non-blocking agent turn dispatch and transcript event application.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use crate::agent::{AgentUiEvent, CodingAgentSession};

use super::transcript::{TranscriptMessage, TranscriptStyle, format_tool_card_content, format_tool_card_result};

/// Spawns agent work on the tokio runtime without blocking the TUI render loop.
pub struct TurnDispatcher;

impl TurnDispatcher {
    pub fn spawn_turn(session: Arc<CodingAgentSession>, text: String, steer: bool) {
        tokio::spawn(async move {
            if let Err(err) = session.submit_prompt(text, steer).await {
                log::error!("agent turn failed: {err}");
            }
        });
    }

    pub fn spawn_abort(session: Arc<CodingAgentSession>) {
        tokio::spawn(async move {
            if let Err(err) = session.abort().await {
                log::warn!("agent abort failed: {err}");
            }
        });
    }
}

/// FIFO queue for follow-up prompts submitted while a turn is in flight.
#[derive(Debug, Default)]
pub struct PromptQueue {
    items: VecDeque<String>,
}

impl PromptQueue {
    pub fn push(&mut self, text: String) {
        if !text.trim().is_empty() {
            self.items.push_back(text);
        }
    }

    pub fn pop_front(&mut self) -> Option<String> {
        self.items.pop_front()
    }
}

/// Applies streaming agent events to transcript messages.
pub struct TranscriptEventApplier {
    live_tool_indexes: HashMap<String, usize>,
    show_thinking: bool,
}

impl TranscriptEventApplier {
    pub fn new(show_thinking: bool) -> Self {
        Self {
            live_tool_indexes: HashMap::new(),
            show_thinking,
        }
    }

    /// Returns `true` when `messages` was mutated.
    pub fn apply(&mut self, messages: &mut Vec<TranscriptMessage>, event: AgentUiEvent) -> bool {
        match event {
            AgentUiEvent::TextDelta(delta) => self.append_assistant(messages, &delta),
            AgentUiEvent::ThinkingDelta(delta) if self.show_thinking => self.append_thinking(messages, &delta),
            AgentUiEvent::ToolStart { id, name, args_summary } => self.start_tool(messages, id, name, args_summary),
            AgentUiEvent::ToolUpdate { id, output } => self.update_tool(messages, &id, &output),
            AgentUiEvent::ToolEnd { id, is_error, output } => self.end_tool(messages, &id, is_error, &output),
            AgentUiEvent::RunCompleted { .. } => self.finalize(messages),
            AgentUiEvent::SubagentStatus {
                agent_id,
                agent_path,
                message,
            } => self.push_status(messages, &format!("[{agent_path} ({agent_id})] {message}")),
            AgentUiEvent::GoalUpdated { objective, status } => {
                if let (Some(objective), Some(status)) = (objective, status) {
                    self.push_status(messages, &format!("Goal ({status}): {objective}"))
                } else {
                    false
                }
            }
            AgentUiEvent::Status(_)
            | AgentUiEvent::ThinkingDelta(_)
            | AgentUiEvent::PlanConfirmationRequired(_)
            | AgentUiEvent::ToolApprovalRequired(_)
            | AgentUiEvent::UserQuestionRequired(_) => false,
        }
    }

    fn push_status(&mut self, messages: &mut Vec<TranscriptMessage>, line: &str) -> bool {
        let line = line.trim();
        if line.is_empty() {
            return false;
        }
        if let Some(last) = messages.last_mut()
            && last.style == TranscriptStyle::Meta
        {
            last.content = line.to_string();
            return true;
        }
        messages.push(TranscriptMessage {
            content: line.to_string(),
            style: TranscriptStyle::Meta,
        });
        true
    }

    fn append_assistant(&mut self, messages: &mut Vec<TranscriptMessage>, delta: &str) -> bool {
        if delta.is_empty() {
            return false;
        }
        if let Some(last) = messages.last_mut()
            && last.style == TranscriptStyle::Assistant
        {
            last.content.push_str(delta);
            return true;
        }
        if let Some(last) = messages.last_mut()
            && last.style == TranscriptStyle::Thinking
        {
            trim_flush_trailing_ws(last);
        }
        messages.push(TranscriptMessage {
            content: delta.to_string(),
            style: TranscriptStyle::Assistant,
        });
        true
    }

    fn append_thinking(&mut self, messages: &mut Vec<TranscriptMessage>, delta: &str) -> bool {
        if delta.is_empty() {
            return false;
        }
        if let Some(last) = messages.last_mut()
            && last.style == TranscriptStyle::Thinking
        {
            last.content.push_str(delta);
            return true;
        }
        messages.push(TranscriptMessage {
            content: delta.to_string(),
            style: TranscriptStyle::Thinking,
        });
        true
    }

    fn start_tool(
        &mut self,
        messages: &mut Vec<TranscriptMessage>,
        id: String,
        name: String,
        args_summary: String,
    ) -> bool {
        let index = messages.len();
        self.live_tool_indexes.insert(id, index);
        messages.push(TranscriptMessage {
            content: format_tool_card_content(&name, &args_summary, true),
            style: TranscriptStyle::ToolRunning,
        });
        true
    }

    fn update_tool(&mut self, messages: &mut [TranscriptMessage], id: &str, output: &str) -> bool {
        if output.is_empty() {
            return false;
        }
        let Some(index) = self.live_tool_indexes.get(id).copied() else {
            return false;
        };
        let Some(message) = messages.get_mut(index) else {
            return false;
        };
        message.content.push_str(output);
        true
    }

    fn end_tool(&mut self, messages: &mut [TranscriptMessage], id: &str, is_error: bool, output: &str) -> bool {
        if let Some(index) = self.live_tool_indexes.remove(id)
            && let Some(message) = messages.get_mut(index)
        {
            message.content = format_tool_card_result(&message.content, is_error, output);
            message.style = if is_error {
                TranscriptStyle::ToolFailed
            } else {
                TranscriptStyle::ToolSuccess
            };
            return true;
        }
        false
    }

    fn finalize(&mut self, messages: &mut [TranscriptMessage]) -> bool {
        self.live_tool_indexes.clear();
        messages.iter().any(|m| m.style == TranscriptStyle::Assistant)
    }
}

fn trim_flush_trailing_ws(message: &mut TranscriptMessage) {
    let trimmed = message.content.trim_end();
    if trimmed.len() != message.content.len() {
        message.content = trimmed.to_string();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_deltas_append_to_streaming_assistant() {
        let mut messages = Vec::new();
        let mut applier = TranscriptEventApplier::new(false);
        assert!(applier.apply(&mut messages, AgentUiEvent::TextDelta("Hel".into())));
        assert!(applier.apply(&mut messages, AgentUiEvent::TextDelta("lo".into())));
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].content, "Hello");
    }

    #[test]
    fn tool_card_transitions_running_to_success() {
        let mut messages = Vec::new();
        let mut applier = TranscriptEventApplier::new(false);
        applier.apply(
            &mut messages,
            AgentUiEvent::ToolStart {
                id: "t1".into(),
                name: "read_file".into(),
                args_summary: "main.rs".into(),
            },
        );
        assert_eq!(messages[0].style, TranscriptStyle::ToolRunning);
        applier.apply(
            &mut messages,
            AgentUiEvent::ToolEnd {
                id: "t1".into(),
                is_error: false,
                output: String::new(),
            },
        );
        assert_eq!(messages[0].style, TranscriptStyle::ToolSuccess);
        assert!(messages[0].content.starts_with('●'));
    }

    #[test]
    fn tool_card_transitions_running_to_failed() {
        let mut messages = Vec::new();
        let mut applier = TranscriptEventApplier::new(false);
        applier.apply(
            &mut messages,
            AgentUiEvent::ToolStart {
                id: "t2".into(),
                name: "bash".into(),
                args_summary: "npm test".into(),
            },
        );
        applier.apply(
            &mut messages,
            AgentUiEvent::ToolEnd {
                id: "t2".into(),
                is_error: true,
                output: "exit 1".into(),
            },
        );
        assert_eq!(messages[0].style, TranscriptStyle::ToolFailed);
        assert!(messages[0].content.starts_with('✕'));
    }

    #[test]
    fn assistant_start_trims_trailing_whitespace_from_thinking() {
        let mut messages = vec![TranscriptMessage {
            content: "thinking line\n\n".to_string(),
            style: TranscriptStyle::Thinking,
        }];
        let mut applier = TranscriptEventApplier::new(true);
        applier.apply(&mut messages, AgentUiEvent::TextDelta("Hello".into()));
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].content, "thinking line");
        assert_eq!(messages[1].content, "Hello");
    }

    #[test]
    fn prompt_queue_skips_empty() {
        let mut queue = PromptQueue::default();
        queue.push("   ".into());
        assert!(queue.pop_front().is_none());
        queue.push("next".into());
        assert_eq!(queue.pop_front().as_deref(), Some("next"));
    }
}
