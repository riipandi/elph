//! Applies [`AgentUiEvent`] updates to [`TranscriptEntry`] lists.

use std::collections::HashMap;

use elph_tui::{
    DEFAULT_TRANSCRIPT_CAP, ToolExecutionState, ToolExecutionStatus, TranscriptEntry, TranscriptRole, push_capped,
};

use crate::agent::AgentUiEvent;

pub struct TranscriptApplier<'a> {
    entries: &'a mut Vec<TranscriptEntry>,
    live_tools: &'a mut Vec<ToolExecutionState>,
    tool_indexes: HashMap<String, usize>,
    show_thinking: bool,
}

impl<'a> TranscriptApplier<'a> {
    pub fn new(
        entries: &'a mut Vec<TranscriptEntry>,
        live_tools: &'a mut Vec<ToolExecutionState>,
        show_thinking: bool,
    ) -> Self {
        let mut tool_indexes = HashMap::new();
        for (index, tool) in live_tools.iter().enumerate() {
            tool_indexes.insert(tool.id.clone(), index);
        }
        Self {
            entries,
            live_tools,
            tool_indexes,
            show_thinking,
        }
    }

    pub fn apply(&mut self, event: AgentUiEvent) {
        match event {
            AgentUiEvent::Status(line) => self.push_status(&line),
            AgentUiEvent::TextDelta(delta) => self.append_assistant_text(&delta),
            AgentUiEvent::ThinkingDelta(delta) if self.show_thinking => self.append_thinking(&delta),
            AgentUiEvent::ToolStart { id, name, args_summary } => self.start_tool(id, name, args_summary),
            AgentUiEvent::ToolUpdate { id, output } => self.update_tool_output(&id, &output),
            AgentUiEvent::ToolEnd { id, is_error, output } => self.end_tool(&id, is_error, &output),
            AgentUiEvent::RunCompleted { .. } => self.finalize_streaming(),
            AgentUiEvent::SubagentStatus {
                agent_id,
                agent_path,
                message,
            } => {
                self.push_status(&format!("[{agent_path} ({agent_id})] {message}"));
            }
            AgentUiEvent::GoalUpdated { objective, status } => {
                if let (Some(objective), Some(status)) = (objective, status) {
                    self.push_status(&format!("Goal ({status}): {objective}"));
                }
            }
            AgentUiEvent::PlanConfirmationRequired(_) | AgentUiEvent::ToolApprovalRequired(_) => {}
            AgentUiEvent::ThinkingDelta(_) => {}
        }
    }

    fn push_capped(&mut self, entry: TranscriptEntry) {
        push_capped(self.entries, entry, DEFAULT_TRANSCRIPT_CAP);
    }

    fn push_status(&mut self, line: &str) {
        let line = line.trim();
        if line.is_empty() {
            return;
        }
        if let Some(last) = self.entries.last_mut()
            && last.role == TranscriptRole::System
        {
            last.content = line.to_string();
            return;
        }
        self.push_capped(TranscriptEntry::system(line));
    }

    fn append_assistant_text(&mut self, delta: &str) {
        if delta.is_empty() {
            return;
        }
        if let Some(last) = self.entries.last_mut()
            && last.role == TranscriptRole::Assistant
            && last.is_streaming
        {
            last.content.push_str(delta);
            return;
        }
        self.push_capped(TranscriptEntry {
            role: TranscriptRole::Assistant,
            content: delta.to_string(),
            is_streaming: true,
            tool: None,
            thinking_expanded: false,
            timestamp: None,
        });
    }

    fn append_thinking(&mut self, delta: &str) {
        if delta.is_empty() {
            return;
        }
        if let Some(last) = self.entries.last_mut()
            && last.role == TranscriptRole::Thinking
        {
            last.content.push_str(delta);
            return;
        }
        self.push_capped(TranscriptEntry::thinking(delta, false));
    }

    fn start_tool(&mut self, id: String, name: String, args_summary: String) {
        let state = ToolExecutionState::new(id.clone(), name)
            .with_args(args_summary)
            .with_status(ToolExecutionStatus::Running);
        self.tool_indexes.insert(id, self.live_tools.len());
        self.live_tools.push(state);
    }

    fn update_tool_output(&mut self, id: &str, output: &str) {
        if let Some(index) = self.tool_indexes.get(id).copied()
            && let Some(tool) = self.live_tools.get_mut(index)
        {
            tool.output.push_str(output);
        }
    }

    fn end_tool(&mut self, id: &str, is_error: bool, output: &str) {
        let finished = if let Some(index) = self.tool_indexes.get(id).copied()
            && let Some(tool) = self.live_tools.get_mut(index)
        {
            if !output.is_empty() {
                tool.output = output.to_string();
            }
            tool.status = if is_error {
                ToolExecutionStatus::Error
            } else {
                ToolExecutionStatus::Success
            };
            Some(tool.clone())
        } else {
            None
        };
        if let Some(tool) = finished {
            self.push_capped(TranscriptEntry::tool(tool));
        }
    }

    fn finalize_streaming(&mut self) {
        for entry in self.entries.iter_mut() {
            if entry.role == TranscriptRole::Assistant {
                entry.is_streaming = false;
            }
        }
        self.live_tools.clear();
        self.tool_indexes.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use elph_tui::{ToolExecutionState, ToolExecutionStatus};

    fn applier<'a>(
        entries: &'a mut Vec<TranscriptEntry>,
        live_tools: &'a mut Vec<ToolExecutionState>,
    ) -> TranscriptApplier<'a> {
        TranscriptApplier::new(entries, live_tools, false)
    }

    #[test]
    fn text_deltas_append_to_streaming_assistant() {
        let mut entries = Vec::new();
        let mut live_tools = Vec::new();
        let mut applier = applier(&mut entries, &mut live_tools);
        applier.apply(AgentUiEvent::TextDelta("Hel".into()));
        applier.apply(AgentUiEvent::TextDelta("lo".into()));
        assert_eq!(entries.len(), 1);
        assert!(entries[0].is_streaming);
        assert_eq!(entries[0].content, "Hello");
    }

    #[test]
    fn run_completed_clears_streaming_flag() {
        let mut entries = vec![TranscriptEntry {
            role: TranscriptRole::Assistant,
            content: "Hi".into(),
            is_streaming: true,
            tool: None,
            thinking_expanded: false,
            timestamp: None,
        }];
        let mut live_tools = vec![ToolExecutionState::new("x", "bash").with_status(ToolExecutionStatus::Running)];
        let mut applier = applier(&mut entries, &mut live_tools);
        applier.apply(AgentUiEvent::RunCompleted { elapsed_secs: 1.2 });
        assert!(!entries[0].is_streaming);
        assert!(live_tools.is_empty());
    }
}
