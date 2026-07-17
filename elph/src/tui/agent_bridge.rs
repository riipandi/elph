//! Non-blocking agent turn dispatch and transcript event application.

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use crate::agent::format_skill_conflict_notice;
use crate::agent::goal_slash::handle_goal_slash;
use crate::agent::{AgentUiEvent, CodingAgentSession, SlashDispatch, SubagentUiPhase};
use crate::extensions::ExtensionHost;
use crate::platform::Paths;

use super::activity::normalize_agent_status;
use super::chrome::format_elapsed_secs;
use super::subagent_display::{
    format_subagent_status_detail, format_subagent_task_label, subagent_status_indent, subagent_status_key,
};
use super::transcript::markdown::AssistantMarkdownBuffer;
use super::transcript::{TranscriptMessage, TranscriptStyle};

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

/// Runs wired slash commands on the agent session and reports via UI events.
pub struct SlashDispatcher;

impl SlashDispatcher {
    pub fn spawn(
        session: Arc<CodingAgentSession>,
        dispatch: SlashDispatch,
        extension_host: Option<ExtensionHost>,
        paths: Option<Paths>,
        cwd: Option<PathBuf>,
    ) {
        tokio::spawn(async move {
            let ui_tx = session.ui_event_sender();
            match dispatch {
                SlashDispatch::Compact => {
                    let status = match session.compact().await {
                        Ok(_) => "History compacted.".into(),
                        Err(err) => format!("Compact failed: {err}"),
                    };
                    let _ = ui_tx.send(AgentUiEvent::Status(status));
                }
                SlashDispatch::Goal { args } => {
                    let status = match handle_goal_slash(session.goal_runtime().as_ref(), &args).await {
                        Ok(message) => message,
                        Err(err) => format!("Goal error: {err}"),
                    };
                    let _ = ui_tx.send(AgentUiEvent::Status(status));
                }
                SlashDispatch::Reload => {
                    let mut messages = Vec::new();
                    if let (Some(paths), Some(cwd)) = (paths.as_ref(), cwd.as_ref()) {
                        match session.reload_resources(paths, cwd).await {
                            Ok(loaded) => {
                                messages.push("Resources reloaded.".into());
                                if let Some(notice) = format_skill_conflict_notice(&loaded.skill_conflicts) {
                                    messages.push(notice);
                                }
                            }
                            Err(err) => messages.push(format!("Resource reload failed: {err}")),
                        }
                    }
                    if let Some(host) = extension_host.as_ref()
                        && let Some(paths) = paths.as_ref()
                    {
                        match host.reload(paths, true) {
                            Ok(_) => messages.push("Extensions reloaded.".into()),
                            Err(err) => messages.push(format!("Extension reload failed: {err}")),
                        }
                    }
                    if messages.is_empty() {
                        messages.push("Reload unavailable.".into());
                    }
                    let _ = ui_tx.send(AgentUiEvent::Status(messages.join("\n\n")));
                }
                SlashDispatch::Extension { name, args } => {
                    let status = if let Some(host) = extension_host.as_ref() {
                        match host.dispatch_slash(&name, &args) {
                            Some(Ok(result)) if result.is_error => format!("Extension error: {}", result.message),
                            Some(Ok(result)) => result.message,
                            Some(Err(err)) => format!("Extension error: {err}"),
                            None => format!("Extension command not found: /{name}"),
                        }
                    } else {
                        "Extension host unavailable.".into()
                    };
                    let _ = ui_tx.send(AgentUiEvent::Status(status));
                }
                SlashDispatch::PromptTemplate { name, args } => {
                    if let Err(err) = session.prompt_from_template(&name, &args).await {
                        let _ = ui_tx.send(AgentUiEvent::Status(format!("Template error: {err}")));
                    }
                }
                SlashDispatch::Skill { name, args } => {
                    if let Err(err) = session.invoke_skill(&name, &args).await {
                        log::error!("skill dispatch failed ({name}): {err}");
                    }
                }
                SlashDispatch::Quit
                | SlashDispatch::Help
                | SlashDispatch::Tools { .. }
                | SlashDispatch::SystemPrompt
                | SlashDispatch::Confetti { .. }
                | SlashDispatch::Unimplemented(_)
                | SlashDispatch::OverlayNeeded(_) => {}
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

    pub fn clear(&mut self) {
        self.items.clear();
    }
}

/// Applies streaming agent events to transcript messages.
pub struct TranscriptEventApplier {
    live_tool_indexes: HashMap<String, usize>,
    tool_started_at: HashMap<String, Instant>,
    thinking_started_at: Option<Instant>,
    assistant_started_at: Option<Instant>,
    meta_started_at: Option<Instant>,
    show_thinking: bool,
    auto_expand_thinking: bool,
}

impl TranscriptEventApplier {
    pub fn new(show_thinking: bool, auto_expand_thinking: bool) -> Self {
        Self {
            live_tool_indexes: HashMap::new(),
            tool_started_at: HashMap::new(),
            thinking_started_at: None,
            assistant_started_at: None,
            meta_started_at: None,
            show_thinking,
            auto_expand_thinking,
        }
    }

    fn finalize_thinking(&mut self, messages: &mut [TranscriptMessage]) {
        let Some(started) = self.thinking_started_at.take() else {
            return;
        };
        if let Some(index) = last_message_index(messages, TranscriptStyle::Thinking) {
            messages[index].duration_secs = Some(format_elapsed_secs(started));
            // Collapse by default so the transcript stays compact after the stream ends.
            messages[index].detail_expanded = self.auto_expand_thinking;
        }
    }

    fn finalize_assistant(&mut self, messages: &mut [TranscriptMessage]) {
        let Some(started) = self.assistant_started_at.take() else {
            return;
        };
        if let Some(index) = last_message_index(messages, TranscriptStyle::Assistant) {
            messages[index].duration_secs = Some(format_elapsed_secs(started));
        }
    }

    fn finalize_meta(&mut self, messages: &mut [TranscriptMessage]) {
        let Some(started) = self.meta_started_at.take() else {
            return;
        };
        if let Some(index) = last_message_index(messages, TranscriptStyle::Meta) {
            messages[index].duration_secs = Some(format_elapsed_secs(started));
        }
    }

    fn begin_thinking(&mut self, messages: &mut [TranscriptMessage]) {
        self.finalize_meta(messages);
        self.thinking_started_at = Some(Instant::now());
    }

    fn begin_assistant(&mut self, messages: &mut [TranscriptMessage]) {
        self.finalize_thinking(messages);
        self.finalize_meta(messages);
        self.assistant_started_at = Some(Instant::now());
    }

    fn begin_meta(&mut self, messages: &mut [TranscriptMessage]) {
        self.finalize_meta(messages);
        self.meta_started_at = Some(Instant::now());
    }

    /// Returns `true` when `messages` was mutated.
    pub fn apply(&mut self, messages: &mut Vec<TranscriptMessage>, event: AgentUiEvent) -> bool {
        match event {
            AgentUiEvent::TextDelta(delta) => self.append_assistant(messages, &delta),
            AgentUiEvent::ThinkingDelta(delta) if self.show_thinking => self.append_thinking(messages, &delta),
            AgentUiEvent::ToolStart { id, name, args_summary } => self.start_tool(messages, id, name, args_summary),
            AgentUiEvent::ToolUpdate { id, output } => self.update_tool(messages, &id, &output),
            AgentUiEvent::ToolEnd { id, is_error, output } => self.end_tool(messages, &id, is_error, &output),
            AgentUiEvent::RunCompleted { .. } => self.finalize_turn(messages),
            AgentUiEvent::SubagentStatus {
                agent_id,
                agent_path,
                task_name,
                phase,
                message,
            } => self.upsert_subagent_status(messages, &agent_id, &agent_path, &task_name, phase, &message),
            AgentUiEvent::GoalUpdated { objective, status } => {
                if let (Some(objective), Some(status)) = (objective, status) {
                    self.push_status(messages, &format!("Goal ({status}): {objective}"))
                } else {
                    false
                }
            }
            AgentUiEvent::Status(message) => self.push_status(messages, message.trim()),
            AgentUiEvent::ThinkingDelta(_)
            | AgentUiEvent::PlanConfirmationRequired(_)
            | AgentUiEvent::UserQuestionRequired(_) => false,
            // ToolApprovalRequired is handled in shell (must respond on response_tx).
            AgentUiEvent::ToolApprovalRequired(_) => false,
        }
    }

    fn push_status(&mut self, messages: &mut Vec<TranscriptMessage>, line: &str) -> bool {
        let line = line.trim();
        if line.is_empty() {
            return false;
        }
        // Ephemeral turn activity (spinner + status row) must not become a meta transcript card.
        if normalize_agent_status(line) == "Thinking" {
            return false;
        }
        if let Some(last) = messages.last_mut()
            && last.style == TranscriptStyle::Meta
        {
            last.content = line.to_string();
            return true;
        }
        self.begin_meta(messages);
        messages.push(TranscriptMessage::text(line, TranscriptStyle::Meta));
        true
    }

    /// Upsert one process-status row per subagent (glyph + role + short name + action + phase word).
    fn upsert_subagent_status(
        &mut self,
        messages: &mut Vec<TranscriptMessage>,
        agent_id: &str,
        agent_path: &str,
        task_name: &str,
        phase: SubagentUiPhase,
        message: &str,
    ) -> bool {
        let key = subagent_status_key(agent_id);
        // Task title (bold when finished) vs action/phase detail (always normal weight).
        // Nesting indents the whole glyph+label row — not leading spaces in the title.
        let content = format_subagent_task_label(task_name, agent_path, agent_id);
        let status_detail = format_subagent_status_detail(message, phase);
        let status_indent = subagent_status_indent(agent_path);
        let style = match phase {
            SubagentUiPhase::Pending | SubagentUiPhase::Running => TranscriptStyle::StatusRunning,
            SubagentUiPhase::Idle | SubagentUiPhase::Done => TranscriptStyle::StatusSuccess,
            SubagentUiPhase::Error => TranscriptStyle::StatusFailed,
        };
        if let Some(existing) = messages
            .iter_mut()
            .find(|message| message.startup_key.as_deref() == Some(key.as_str()))
        {
            existing.content = content;
            existing.status_detail = Some(status_detail);
            existing.status_indent = status_indent;
            existing.style = style;
            return true;
        }
        let mut row = TranscriptMessage::startup_status(key, content, style);
        row.status_detail = Some(status_detail);
        row.status_indent = status_indent;
        messages.push(row);
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
        self.begin_assistant(messages);
        let mut message = TranscriptMessage::text(delta, TranscriptStyle::Assistant);
        message.markdown = Some(AssistantMarkdownBuffer::new());
        messages.push(message);
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
        self.begin_thinking(messages);
        messages.push(TranscriptMessage::text(delta, TranscriptStyle::Thinking));
        true
    }

    fn start_tool(
        &mut self,
        messages: &mut Vec<TranscriptMessage>,
        id: String,
        name: String,
        args_summary: String,
    ) -> bool {
        self.finalize_thinking(messages);
        self.finalize_assistant(messages);
        self.finalize_meta(messages);
        let index = messages.len();
        self.live_tool_indexes.insert(id.clone(), index);
        self.tool_started_at.insert(id, Instant::now());
        messages.push(TranscriptMessage::tool_call(name, args_summary, TranscriptStyle::ToolRunning));
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
        if let Some(tool) = message.tool.as_mut() {
            tool.output.push_str(output);
            return true;
        }
        message.content.push_str(output);
        true
    }

    fn end_tool(&mut self, messages: &mut [TranscriptMessage], id: &str, is_error: bool, output: &str) -> bool {
        if let Some(index) = self.live_tool_indexes.remove(id)
            && let Some(message) = messages.get_mut(index)
        {
            if let Some(tool) = message.tool.as_mut()
                && !output.is_empty()
            {
                tool.output = output.to_string();
            }
            message.style = if is_error {
                TranscriptStyle::ToolFailed
            } else {
                TranscriptStyle::ToolSuccess
            };
            if let Some(started) = self.tool_started_at.remove(id) {
                message.duration_secs = Some(format_elapsed_secs(started));
            }
            // Collapse args/output so the transcript stays compact after the tool finishes.
            message.detail_expanded = false;
            return true;
        }
        false
    }

    fn finalize_turn(&mut self, messages: &mut [TranscriptMessage]) -> bool {
        self.finalize_assistant(messages);
        self.finalize_meta(messages);
        self.live_tool_indexes.clear();
        self.tool_started_at.clear();
        let Some(last) = messages.last_mut() else {
            return false;
        };
        if last.style != TranscriptStyle::Assistant {
            return false;
        }
        trim_flush_trailing_ws(last);
        if last.markdown.is_none() {
            last.markdown = Some(AssistantMarkdownBuffer::new());
        }
        if let Some(markdown) = last.markdown.as_mut() {
            markdown.mark_stream_complete();
        }
        true
    }
}

fn last_message_index(messages: &[TranscriptMessage], style: TranscriptStyle) -> Option<usize> {
    messages.iter().rposition(|message| message.style == style)
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
        let mut applier = TranscriptEventApplier::new(false, false);
        assert!(applier.apply(&mut messages, AgentUiEvent::TextDelta("Hel".into())));
        assert!(applier.apply(&mut messages, AgentUiEvent::TextDelta("lo".into())));
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].content, "Hello");
        assert!(messages[0].markdown.is_some());
    }

    #[test]
    fn run_completed_marks_assistant_markdown_stream_complete() {
        let mut messages = Vec::new();
        let mut applier = TranscriptEventApplier::new(false, false);
        applier.apply(&mut messages, AgentUiEvent::TextDelta("Hi\n\n".into()));
        applier.apply(&mut messages, AgentUiEvent::TextDelta("Done.".into()));
        assert!(applier.apply(&mut messages, AgentUiEvent::RunCompleted { elapsed_secs: 0.0 }));
        let markdown = messages[0].markdown.as_ref().expect("markdown buffer");
        assert!(markdown.stream_complete);
    }

    #[test]
    fn tool_card_transitions_running_to_success() {
        let mut messages = Vec::new();
        let mut applier = TranscriptEventApplier::new(false, false);
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
        assert_eq!(messages[0].tool.as_ref().unwrap().name, "read_file");
    }

    #[test]
    fn tool_card_transitions_running_to_failed() {
        let mut messages = Vec::new();
        let mut applier = TranscriptEventApplier::new(false, false);
        applier.apply(
            &mut messages,
            AgentUiEvent::ToolStart {
                id: "t2".into(),
                name: "shell_exec".into(),
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
        assert_eq!(messages[0].tool.as_ref().unwrap().output, "exit 1");
    }

    #[test]
    fn tool_update_streams_output_into_card_body() {
        let mut messages = Vec::new();
        let mut applier = TranscriptEventApplier::new(false, false);
        applier.apply(
            &mut messages,
            AgentUiEvent::ToolStart {
                id: "t3".into(),
                name: "shell_exec".into(),
                args_summary: r#"{"command":"cargo test"}"#.into(),
            },
        );
        applier.apply(
            &mut messages,
            AgentUiEvent::ToolUpdate {
                id: "t3".into(),
                output: "running 1 test".into(),
            },
        );
        assert_eq!(messages[0].tool.as_ref().unwrap().output, "running 1 test");
    }

    #[test]
    fn status_events_become_meta_lines() {
        let mut messages = Vec::new();
        let mut applier = TranscriptEventApplier::new(false, false);
        assert!(applier.apply(&mut messages, AgentUiEvent::Status("History compacted.".into())));
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].style, TranscriptStyle::Meta);
        assert_eq!(messages[0].content, "History compacted.");
    }

    #[test]
    fn thinking_status_stays_out_of_transcript() {
        let mut messages = Vec::new();
        let mut applier = TranscriptEventApplier::new(false, false);
        assert!(!applier.apply(&mut messages, AgentUiEvent::Status("Thinking…".into())));
        assert!(messages.is_empty());
    }

    #[test]
    fn subagent_status_upserts_process_row_with_accessible_label() {
        use crate::agent::SubagentUiPhase;

        let mut messages = Vec::new();
        let mut applier = TranscriptEventApplier::new(false, false);
        assert!(applier.apply(
            &mut messages,
            AgentUiEvent::SubagentStatus {
                agent_id: "agent_01".into(),
                agent_path: "main/worker-1".into(),
                task_name: "worker-1".into(),
                phase: SubagentUiPhase::Running,
                message: "tool:read_file".into(),
            },
        ));
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].style, TranscriptStyle::StatusRunning);
        assert_eq!(messages[0].startup_key.as_deref(), Some("subagent:agent_01"));
        assert!(messages[0].content.contains("Subagent worker-1"), "{}", messages[0].content);
        assert!(
            !messages[0].content.contains("Read"),
            "task title only: {}",
            messages[0].content
        );
        assert_eq!(
            messages[0].status_detail.as_deref(),
            Some("Read · running"),
            "detail holds action/phase"
        );

        // Same agent tool update upserts in place (low noise).
        assert!(applier.apply(
            &mut messages,
            AgentUiEvent::SubagentStatus {
                agent_id: "agent_01".into(),
                agent_path: "main/worker-1".into(),
                task_name: "worker-1".into(),
                phase: SubagentUiPhase::Running,
                message: "tool:shell_exec".into(),
            },
        ));
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].status_detail.as_deref(), Some("Shell · running"));

        assert!(applier.apply(
            &mut messages,
            AgentUiEvent::SubagentStatus {
                agent_id: "agent_01".into(),
                agent_path: "main/worker-1".into(),
                task_name: "worker-1".into(),
                phase: SubagentUiPhase::Done,
                message: String::new(),
            },
        ));
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].style, TranscriptStyle::StatusSuccess);
        assert_eq!(messages[0].status_detail.as_deref(), Some("done"));
    }

    #[test]
    fn nested_subagent_gets_indented_status_label() {
        use crate::agent::SubagentUiPhase;

        let mut messages = Vec::new();
        let mut applier = TranscriptEventApplier::new(false, false);
        applier.apply(
            &mut messages,
            AgentUiEvent::SubagentStatus {
                agent_id: "child".into(),
                agent_path: "main/a/b".into(),
                task_name: String::new(),
                phase: SubagentUiPhase::Running,
                message: String::new(),
            },
        );
        // Nesting pads the whole row (glyph+label); the title itself stays flush for tight glyph gap.
        assert!(!messages[0].content.starts_with(' '), "{}", messages[0].content);
        assert!(messages[0].content.starts_with("Subagent"), "{}", messages[0].content);
        assert_eq!(messages[0].status_indent, 2); // depth 2 → one nest past main children
    }

    #[test]
    fn tool_end_records_duration_secs() {
        let mut messages = Vec::new();
        let mut applier = TranscriptEventApplier::new(false, false);
        applier.apply(
            &mut messages,
            AgentUiEvent::ToolStart {
                id: "t-dur".into(),
                name: "grep".into(),
                args_summary: "pattern".into(),
            },
        );
        std::thread::sleep(std::time::Duration::from_millis(120));
        applier.apply(
            &mut messages,
            AgentUiEvent::ToolEnd {
                id: "t-dur".into(),
                is_error: false,
                output: String::new(),
            },
        );
        assert!(messages[0].duration_secs.is_some_and(|secs| secs > 0.0));
        assert!(!messages[0].detail_expanded);
        assert!(messages[0].is_tool_collapsed());
    }

    #[test]
    fn thinking_records_duration_when_response_starts() {
        let mut messages = Vec::new();
        let mut applier = TranscriptEventApplier::new(true, false);
        applier.apply(&mut messages, AgentUiEvent::ThinkingDelta("plan".into()));
        std::thread::sleep(std::time::Duration::from_millis(120));
        applier.apply(&mut messages, AgentUiEvent::TextDelta("Hi".into()));
        assert!(messages[0].duration_secs.is_some_and(|secs| secs > 0.0));
        assert!(!messages[0].detail_expanded);
        assert!(messages[0].is_thinking_collapsed());
        assert!(messages[1].duration_secs.is_none());
    }

    #[test]
    fn thinking_stays_expanded_when_auto_expand_enabled() {
        let mut messages = Vec::new();
        let mut applier = TranscriptEventApplier::new(true, true);
        applier.apply(&mut messages, AgentUiEvent::ThinkingDelta("plan".into()));
        applier.apply(&mut messages, AgentUiEvent::TextDelta("Hi".into()));
        assert!(messages[0].duration_secs.is_some());
        assert!(messages[0].detail_expanded);
        assert!(!messages[0].is_thinking_collapsed());
    }

    #[test]
    fn assistant_records_duration_on_run_completed() {
        let mut messages = Vec::new();
        let mut applier = TranscriptEventApplier::new(false, false);
        applier.apply(&mut messages, AgentUiEvent::TextDelta("Done".into()));
        std::thread::sleep(std::time::Duration::from_millis(120));
        applier.apply(&mut messages, AgentUiEvent::RunCompleted { elapsed_secs: 1.0 });
        assert!(messages[0].duration_secs.is_some_and(|secs| secs > 0.0));
    }

    #[test]
    fn assistant_start_trims_trailing_whitespace_from_thinking() {
        let mut messages = vec![TranscriptMessage::text("thinking line\n\n", TranscriptStyle::Thinking)];
        let mut applier = TranscriptEventApplier::new(true, false);
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

    #[test]
    fn prompt_queue_clear_drops_queued_turns() {
        let mut queue = PromptQueue::default();
        queue.push("one".into());
        queue.push("two".into());
        queue.clear();
        assert!(queue.pop_front().is_none());
    }
}
