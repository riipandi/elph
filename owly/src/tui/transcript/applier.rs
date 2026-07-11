use std::collections::HashMap;

use elph_tui::{DEFAULT_TRANSCRIPT_CAP, ToolExecutionState, ToolExecutionStatus, TranscriptRole, push_capped};

use crate::ui_events::AgentUiEvent;

use crate::tui::entries::{OwlyEntry, OwlyEntryKind, command_result_entry};

/// Mutable transcript state for one agent run.
pub struct TranscriptApplier<'a> {
    entries: &'a mut Vec<OwlyEntry>,
    live_tools: &'a mut Vec<ToolExecutionState>,
    tool_indexes: HashMap<String, usize>,
    show_thinking: bool,
}

impl<'a> TranscriptApplier<'a> {
    pub fn new(
        entries: &'a mut Vec<OwlyEntry>,
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
            AgentUiEvent::CommandStart {
                command,
                provider,
                model,
            } => self.push_command_start(&command, &provider, &model),
            AgentUiEvent::CommandComplete { message, success } => self.push_command_complete(&message, success),
            AgentUiEvent::Status(line) => self.push_status(&line),
            AgentUiEvent::TextDelta(delta) => self.append_assistant_text(&delta),
            AgentUiEvent::ThinkingDelta(delta) if self.show_thinking => self.append_thinking(&delta),
            AgentUiEvent::ToolStart { id, name, args_summary } => self.start_tool(id, name, args_summary),
            AgentUiEvent::ToolUpdate { id, output } => self.update_tool_output(&id, &output),
            AgentUiEvent::ToolEnd { id, is_error, output } => self.end_tool(&id, is_error, &output),
            AgentUiEvent::RunCompleted { elapsed_secs } => self.complete_run(elapsed_secs),
            AgentUiEvent::AskUserRequired { .. } => {}
            AgentUiEvent::ThinkingDelta(_) => {}
        }
    }

    fn push_capped(&mut self, entry: OwlyEntry) {
        push_capped(self.entries, entry, DEFAULT_TRANSCRIPT_CAP);
    }

    fn push_command_start(&mut self, command: &str, _provider: &str, _model: &str) {
        let _ = command;
    }

    fn push_command_complete(&mut self, message: &str, success: bool) {
        if message.is_empty() {
            return;
        }
        self.push_capped(command_result_entry(message, success));
    }

    fn push_status(&mut self, line: &str) {
        let line = line.trim();
        if line.is_empty() {
            return;
        }
        if let Some(last) = self.entries.last_mut()
            && last.kind == OwlyEntryKind::Status
        {
            last.inner.content = line.to_string();
            return;
        }
        self.push_capped(OwlyEntry::status(line));
    }

    fn append_assistant_text(&mut self, delta: &str) {
        if delta.is_empty() {
            return;
        }
        if let Some(last) = self.entries.last_mut()
            && last.kind == OwlyEntryKind::Assistant
            && last.inner.is_streaming
        {
            last.inner.content.push_str(delta);
            return;
        }
        self.push_capped(OwlyEntry::assistant_streaming(delta));
    }

    fn append_thinking(&mut self, delta: &str) {
        if delta.is_empty() {
            return;
        }
        if let Some(last) = self.entries.last_mut()
            && last.kind == OwlyEntryKind::Thinking
        {
            last.inner.content.push_str(delta);
            return;
        }
        self.push_capped(OwlyEntry::thinking(delta));
    }

    fn start_tool(&mut self, id: String, name: String, args_summary: String) {
        let state = ToolExecutionState::new(id.clone(), name)
            .with_args(args_summary)
            .with_status(ToolExecutionStatus::Running);
        self.tool_indexes.insert(id, self.live_tools.len());
        self.live_tools.push(state);
    }

    fn update_tool_output(&mut self, id: &str, output: &str) {
        let Some(&idx) = self.tool_indexes.get(id) else {
            return;
        };
        let Some(tool) = self.live_tools.get_mut(idx) else {
            return;
        };
        tool.output = output.to_string();
    }

    fn end_tool(&mut self, id: &str, is_error: bool, output: &str) {
        let Some(idx) = self.tool_indexes.remove(id) else {
            return;
        };
        let Some(tool) = self.live_tools.get_mut(idx) else {
            return;
        };
        tool.status = if is_error {
            ToolExecutionStatus::Error
        } else {
            ToolExecutionStatus::Success
        };
        if !output.is_empty() {
            tool.output = output.to_string();
        }
        let summary = tool.clone();
        self.live_tools.remove(idx);
        self.rebuild_tool_indexes();
        self.push_capped(OwlyEntry::tool_summary(summary));
    }

    fn rebuild_tool_indexes(&mut self) {
        self.tool_indexes.clear();
        for (index, tool) in self.live_tools.iter().enumerate() {
            self.tool_indexes.insert(tool.id.clone(), index);
        }
    }

    fn complete_run(&mut self, elapsed_secs: f64) {
        for entry in self.entries.iter_mut() {
            if entry.inner.role == TranscriptRole::Assistant {
                entry.inner.is_streaming = false;
            }
        }
        self.live_tools.clear();
        self.tool_indexes.clear();
        let _ = elapsed_secs;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::transcript::lines_to_entries;

    fn applier<'a>(
        entries: &'a mut Vec<OwlyEntry>,
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
        assert!(entries[0].inner.is_streaming);
        assert_eq!(entries[0].inner.content, "Hello");
    }

    #[test]
    fn command_start_does_not_add_transcript_noise() {
        let mut entries = Vec::new();
        let mut live_tools = Vec::new();
        let mut applier = applier(&mut entries, &mut live_tools);
        applier.apply(AgentUiEvent::CommandStart {
            command: "Init".into(),
            provider: "opencode".into(),
            model: "big-pickle".into(),
        });
        assert!(entries.is_empty());
    }

    #[test]
    fn command_complete_renders_success_marker() {
        let mut entries = Vec::new();
        let mut live_tools = Vec::new();
        let mut applier = applier(&mut entries, &mut live_tools);
        applier.apply(AgentUiEvent::CommandComplete {
            message: "Documentation updated.".into(),
            success: true,
        });
        assert_eq!(entries[0].inner.content, "✓ Documentation updated.");
    }

    #[test]
    fn tool_lifecycle_keeps_running_in_live_panel_until_end() {
        let mut entries = Vec::new();
        let mut live_tools = Vec::new();
        {
            let mut applier = applier(&mut entries, &mut live_tools);
            applier.apply(AgentUiEvent::ToolStart {
                id: "t1".into(),
                name: "bash".into(),
                args_summary: "ls".into(),
            });
        }
        assert_eq!(live_tools.len(), 1);
        assert!(entries.is_empty());

        {
            let mut applier = applier(&mut entries, &mut live_tools);
            applier.apply(AgentUiEvent::ToolUpdate {
                id: "t1".into(),
                output: "file.txt".into(),
            });
        }
        assert_eq!(live_tools[0].output, "file.txt");

        {
            let mut applier = applier(&mut entries, &mut live_tools);
            applier.apply(AgentUiEvent::ToolEnd {
                id: "t1".into(),
                is_error: false,
                output: "file.txt\n".into(),
            });
        }
        assert!(live_tools.is_empty());
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].kind, OwlyEntryKind::ToolSummary);
        assert_eq!(
            entries[0].inner.tool.as_ref().map(|t| t.status),
            Some(ToolExecutionStatus::Success)
        );
    }

    #[test]
    fn status_lines_coalesce() {
        let mut entries = Vec::new();
        let mut live_tools = Vec::new();
        let mut applier = applier(&mut entries, &mut live_tools);
        applier.apply(AgentUiEvent::Status("step 1".into()));
        applier.apply(AgentUiEvent::Status("step 2".into()));
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].inner.content, "step 2");
    }

    #[test]
    fn run_completed_clears_streaming_flag() {
        let mut entries = vec![OwlyEntry::assistant_streaming("Hi")];
        let mut live_tools = vec![ToolExecutionState::new("x", "bash").with_status(ToolExecutionStatus::Running)];
        let mut applier = applier(&mut entries, &mut live_tools);
        applier.apply(AgentUiEvent::RunCompleted { elapsed_secs: 1.2 });
        assert!(!entries[0].inner.is_streaming);
        assert!(live_tools.is_empty());
        assert_eq!(entries.len(), 1);
    }

    #[test]
    fn lines_to_entries_skips_blank_lines() {
        let entries = lines_to_entries(&["hint".into(), String::new(), "  ".into()]);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].kind, OwlyEntryKind::Hint);
    }
}
