//! Non-blocking agent turn dispatch and transcript event application.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use crate::agent::format_skill_conflict_notice;
use crate::agent::goal_slash::handle_goal_slash;
use crate::agent::{AgentUiEvent, CodingAgentSession, QueuedPromptItem, QueuedPromptKind};
use crate::agent::{SlashDispatch, SubagentUiPhase};
use crate::extensions::ExtensionHost;
use crate::platform::Paths;

use super::activity::normalize_agent_status;
use super::chrome::format_elapsed_secs;
use super::subagent_display::{subagent_status_indent, subagent_status_key};
use super::tool_approval::strip_plan_tags;
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

    pub fn spawn_follow_up(session: Arc<CodingAgentSession>, text: String) {
        tokio::spawn(async move {
            if let Err(err) = session.queue_follow_up(text).await {
                log::warn!("queue follow-up failed: {err}");
                let _ = session
                    .ui_event_sender()
                    .send(AgentUiEvent::Status(format!("Could not queue prompt: {err}")));
            }
        });
    }

    pub fn spawn_steer(session: Arc<CodingAgentSession>, text: String) {
        tokio::spawn(async move {
            if let Err(err) = session.queue_steer(text).await {
                log::warn!("queue steer failed: {err}");
                let _ = session
                    .ui_event_sender()
                    .send(AgentUiEvent::Status(format!("Could not interject: {err}")));
            }
        });
    }

    pub fn spawn_remove_queued(session: Arc<CodingAgentSession>, kind: QueuedPromptKind, kind_index: usize) {
        tokio::spawn(async move {
            if let Err(err) = session.remove_queued(kind, kind_index).await {
                log::warn!("remove queued prompt failed: {err}");
            }
        });
    }

    /// Pop one queued item (by kind index) and interject it immediately via steer.
    pub fn spawn_interject_queued(
        session: Arc<CodingAgentSession>,
        kind: QueuedPromptKind,
        kind_index: usize,
        text: String,
    ) {
        tokio::spawn(async move {
            if let Err(err) = session.remove_queued(kind, kind_index).await {
                log::warn!("remove queued before interject failed: {err}");
            }
            if let Err(err) = session.queue_steer(text).await {
                log::warn!("interject queued prompt failed: {err}");
                let _ = session
                    .ui_event_sender()
                    .send(AgentUiEvent::Status(format!("Could not interject: {err}")));
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
                SlashDispatch::NewSession
                | SlashDispatch::Quit
                | SlashDispatch::Help
                | SlashDispatch::Tools { .. }
                | SlashDispatch::SystemPrompt
                | SlashDispatch::SessionInfo
                | SlashDispatch::Rename { .. }
                | SlashDispatch::Confetti { .. }
                | SlashDispatch::Unimplemented(_)
                | SlashDispatch::OverlayNeeded(_)
                | SlashDispatch::Memory { .. } => {}
            }
        });
    }
}

/// Local mirror of harness steer/follow-up queues for StatusRow + Ctrl+Q.
#[derive(Debug, Default, Clone)]
pub struct PromptQueueView {
    items: Vec<QueuedPromptItem>,
    /// Texts removed via **Send** (interject). Hidden until the harness no longer holds them
    /// (Send re-queues as steer, which would otherwise reappear in the list).
    suppressed_sent: Vec<String>,
}

impl PromptQueueView {
    pub fn replace(&mut self, items: Vec<QueuedPromptItem>) {
        // Drop suppress entries that are no longer in the harness snapshot (drained/consumed).
        self.suppressed_sent
            .retain(|text| items.iter().any(|item| item.text == *text));
        self.items = Self::filter_suppressed(items, &self.suppressed_sent);
        self.renumber_seq();
    }

    pub fn items(&self) -> &[QueuedPromptItem] {
        &self.items
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn clear(&mut self) {
        self.items.clear();
        self.suppressed_sent.clear();
    }

    /// Optimistic local append before harness `QueueUpdate` (caller must bump UI revision).
    pub fn push_follow_up_local(&mut self, text: String) {
        let text = text.trim().to_string();
        if text.is_empty() {
            return;
        }
        // New enqueue should not stay hidden if the same string was previously Sent.
        self.suppressed_sent.retain(|t| t != &text);
        let kind_index = self
            .items
            .iter()
            .filter(|i| matches!(i.kind, QueuedPromptKind::FollowUp))
            .count();
        let seq = (self.items.len() as u32).saturating_add(1);
        self.items.push(QueuedPromptItem {
            seq,
            kind: QueuedPromptKind::FollowUp,
            kind_index,
            text,
        });
    }

    /// Remove display index 0 and renumber; returns the removed item.
    pub fn pop_front_local(&mut self) -> Option<QueuedPromptItem> {
        self.remove_at_local(0)
    }

    /// Remove item at display index and renumber; returns the removed item.
    pub fn remove_at_local(&mut self, display_index: usize) -> Option<QueuedPromptItem> {
        if display_index >= self.items.len() {
            return None;
        }
        let removed = self.items.remove(display_index);
        self.renumber_seq();
        Some(removed)
    }

    /// Mark a prompt as Sent/interjected so later `QueueUpdate` snapshots hide it until drained.
    pub fn suppress_sent(&mut self, text: impl Into<String>) {
        let text = text.into();
        if text.is_empty() {
            return;
        }
        if !self.suppressed_sent.iter().any(|t| t == &text) {
            self.suppressed_sent.push(text);
        }
        self.items
            .retain(|item| !self.suppressed_sent.iter().any(|t| t == &item.text));
        self.renumber_seq();
    }

    fn renumber_seq(&mut self) {
        for (i, item) in self.items.iter_mut().enumerate() {
            item.seq = (i as u32).saturating_add(1);
        }
    }

    fn filter_suppressed(items: Vec<QueuedPromptItem>, suppressed: &[String]) -> Vec<QueuedPromptItem> {
        if suppressed.is_empty() {
            return items;
        }
        items
            .into_iter()
            .filter(|item| !suppressed.iter().any(|t| t == &item.text))
            .collect()
    }

    /// Compact badge text, e.g. `Q:3`, or empty when no queue.
    #[cfg(test)]
    pub fn badge_label(&self) -> Option<String> {
        let n = self.len();
        if n == 0 { None } else { Some(format!("Q:{n}")) }
    }
}

/// Legacy alias used by shell quit helpers; same as [`PromptQueueView`].
pub type PromptQueue = PromptQueueView;

/// Merge adjacent high-frequency stream events so one UI tick applies fewer mutations.
///
/// Preserves ordering relative to non-stream events (tool start/end, status, …).
pub fn coalesce_agent_ui_events(events: Vec<AgentUiEvent>) -> Vec<AgentUiEvent> {
    let mut out = Vec::with_capacity(events.len());
    for event in events {
        match event {
            AgentUiEvent::TextDelta(delta) if !delta.is_empty() => {
                if let Some(AgentUiEvent::TextDelta(buf)) = out.last_mut() {
                    buf.push_str(&delta);
                } else {
                    out.push(AgentUiEvent::TextDelta(delta));
                }
            }
            AgentUiEvent::ThinkingDelta(delta) if !delta.is_empty() => {
                if let Some(AgentUiEvent::ThinkingDelta(buf)) = out.last_mut() {
                    buf.push_str(&delta);
                } else {
                    out.push(AgentUiEvent::ThinkingDelta(delta));
                }
            }
            AgentUiEvent::ToolUpdate { id, output } if !output.is_empty() => {
                if let Some(AgentUiEvent::ToolUpdate {
                    id: last_id,
                    output: buf,
                }) = out.last_mut()
                    && last_id == &id
                {
                    buf.push_str(&output);
                } else {
                    out.push(AgentUiEvent::ToolUpdate { id, output });
                }
            }
            other => out.push(other),
        }
    }
    out
}

/// Max bytes kept in streaming tool output. Older bytes are dropped from the front so the
/// card renders the tail without slowdown (matches shell_output.rs buffer cap).
const TOOL_OUTPUT_STREAM_CAP: usize = 100 * 1024;

/// Applies streaming agent events to transcript messages.
pub struct TranscriptEventApplier {
    live_tool_indexes: HashMap<String, usize>,
    tool_started_at: HashMap<String, Instant>,
    thinking_started_at: Option<Instant>,
    assistant_started_at: Option<Instant>,
    meta_started_at: Option<Instant>,
    show_thinking: bool,
    auto_expand_thinking: bool,
    subagent_started_at: HashMap<String, Instant>,
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
            subagent_started_at: HashMap::new(),
        }
    }

    /// Collapse the most recent expanded user-shell tool so new activity has room.
    /// Respects `user_pinned`: if the user manually toggled the card, leave it alone.
    fn fold_user_shell(&mut self, messages: &mut [TranscriptMessage]) {
        if let Some(msg) = messages.iter_mut().rev().find(|m| m.user_shell && !m.user_pinned) {
            msg.detail_expanded = false;
        }
    }

    fn finalize_thinking(&mut self, messages: &mut [TranscriptMessage]) {
        let Some(started) = self.thinking_started_at.take() else {
            return;
        };
        if let Some(index) = last_message_index(messages, TranscriptStyle::Thinking) {
            messages[index].duration_secs = Some(format_elapsed_secs(started));
            // Collapse by default so the transcript stays compact after the stream ends.
            // Respect user_pinned: if the user manually expanded the card, keep it expanded.
            if !messages[index].user_pinned {
                messages[index].detail_expanded = self.auto_expand_thinking;
            }
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
        self.fold_user_shell(messages);
        self.finalize_meta(messages);
        self.thinking_started_at = Some(Instant::now());
    }

    fn begin_assistant(&mut self, messages: &mut [TranscriptMessage]) {
        self.fold_user_shell(messages);
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
            AgentUiEvent::ToolStart {
                id,
                name,
                args_summary,
                user_shell,
            } => self.start_tool(messages, id, name, args_summary, user_shell),
            AgentUiEvent::ToolUpdate { id, output } => self.update_tool(messages, &id, &output),
            AgentUiEvent::ToolEnd {
                id,
                is_error,
                output,
                details,
            } => self.end_tool(messages, &id, is_error, &output, &details),
            AgentUiEvent::RunCompleted { .. } => self.finalize_turn(messages),
            AgentUiEvent::SubagentStatus {
                agent_id,
                agent_path,
                task_name,
                phase,
                message,
                model,
            } => self.upsert_subagent_status(messages, &agent_id, &agent_path, &task_name, phase, &message, &model),
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
            | AgentUiEvent::UserQuestionRequired(_)
            | AgentUiEvent::ModeChangeRequired(_)
            | AgentUiEvent::QueueUpdate { .. }
            | AgentUiEvent::UserPromptCommitted { .. } => false,
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
        // API / provider failures → dedicated error chrome (not a dim meta line).
        if crate::tui::api_error_display::is_user_facing_api_error_line(line) {
            return self.push_api_error(messages, line);
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

    /// Provider/API failure card — red error style, replaces last error if still open.
    fn push_api_error(&mut self, messages: &mut Vec<TranscriptMessage>, line: &str) -> bool {
        use crate::tui::api_error_display::format_user_facing_api_error;
        let line = format_user_facing_api_error(line);
        if line.is_empty() {
            return false;
        }
        // Upsert consecutive API error so MessageEnd + TurnEnd do not double-stack.
        if let Some(last) = messages.last_mut()
            && last.style == TranscriptStyle::Error
        {
            last.content = line;
            return true;
        }
        self.finalize_thinking(messages);
        self.finalize_assistant(messages);
        self.finalize_meta(messages);
        messages.push(TranscriptMessage::text(line, TranscriptStyle::Error));
        true
    }

    /// Upsert one process-status row per subagent (tree format with model and duration).
    #[allow(clippy::too_many_arguments)]
    fn upsert_subagent_status(
        &mut self,
        messages: &mut Vec<TranscriptMessage>,
        agent_id: &str,
        agent_path: &str,
        task_name: &str,
        phase: SubagentUiPhase,
        _message: &str,
        model: &str,
    ) -> bool {
        use std::time::Instant;

        // Track start time on first Running transition for duration measurement.
        let entering_running = phase == SubagentUiPhase::Running
            && !messages
                .iter()
                .any(|m| m.startup_key.as_deref() == Some(&subagent_status_key(agent_id)));
        if entering_running {
            self.subagent_started_at.insert(agent_id.to_string(), Instant::now());
        }

        // Compute duration if agent just finished.
        let finished = matches!(phase, SubagentUiPhase::Done | SubagentUiPhase::Error);
        let duration = finished
            .then(|| {
                self.subagent_started_at
                    .remove(agent_id)
                    .map(crate::tui::chrome::format_elapsed_secs)
            })
            .flatten();

        // Build content (without model) and model_tag separately for colored rendering.
        let name = crate::tui::subagent_display::subagent_short_name(task_name, agent_path, agent_id);
        let model_tag = if model.is_empty() {
            None
        } else {
            Some(model.to_string())
        };
        let agent_tag = Some(agent_id.to_string());
        let task_label = task_name.trim();
        let name_from_task = !task_label.is_empty()
            && !task_label.eq_ignore_ascii_case("default")
            && !task_label.eq_ignore_ascii_case("agent");
        let content = if name_from_task {
            // Name derived from task_name (possibly truncated). Never duplicate task_label.
            name.clone()
        } else if !task_label.is_empty()
            && !task_label.eq_ignore_ascii_case("default")
            && !task_label.eq_ignore_ascii_case("agent")
        {
            // Name came from path tail / agent_id; append task_label for context.
            format!("{name}  {task_label}")
        } else {
            name.clone()
        };

        let indent = subagent_status_indent(agent_path);
        let style = match phase {
            SubagentUiPhase::Pending | SubagentUiPhase::Running => TranscriptStyle::StatusRunning,
            SubagentUiPhase::Idle | SubagentUiPhase::Done => TranscriptStyle::StatusSuccess,
            SubagentUiPhase::Error => TranscriptStyle::StatusFailed,
        };

        // Ensure the subagent header exists (upsert before first subagent).
        self.upsert_subagent_header(messages);

        // Upsert or create the subagent status row.
        let key = subagent_status_key(agent_id);
        if let Some(existing) = messages
            .iter_mut()
            .find(|m| m.startup_key.as_deref() == Some(key.as_str()))
        {
            existing.content = content;
            existing.model_tag = model_tag;
            existing.agent_tag = agent_tag;
            existing.status_detail = None;
            existing.status_indent = indent;
            existing.style = style;
            existing.duration_secs = duration;
            return true;
        }
        let mut row = TranscriptMessage::startup_status(key, content, style);
        row.model_tag = model_tag;
        row.agent_tag = agent_tag;
        row.status_detail = None;
        row.status_indent = indent;
        row.duration_secs = duration;
        messages.push(row);

        // Recalculate tree prefixes now that we have a new row.
        self.rebuild_subagent_tree(messages);
        true
    }

    /// Maintain a single header row ("○ Subagents running") as the first subagent-related message.
    /// Creates it if missing; removes it when no subagent rows remain (handled outside this method).
    fn upsert_subagent_header(&self, messages: &mut Vec<TranscriptMessage>) {
        let has_header = messages
            .iter()
            .any(|m| m.startup_key.as_deref() == Some("subagent:header"));
        if has_header {
            return;
        }
        // Insert header at the start of the subagent block — right after the last non-subagent
        // status row, or at the very end.
        let insert_at = messages
            .iter()
            .rposition(|m| m.startup_key.as_deref().is_some_and(|k| k.starts_with("subagent:")))
            .map(|pos| pos + 1)
            .unwrap_or(messages.len());
        // Status glyph is rendered by the card chrome — content is label-only.
        let mut header =
            TranscriptMessage::startup_status("subagent:header", "Subagents running", TranscriptStyle::StatusRunning);
        header.detail_expanded = true;
        messages.insert(insert_at, header);
    }

    /// Rebuild tree-drawing prefixes (`├─` / `└─`) for all subagent rows based on their
    /// position among sibling subagents (same depth).
    #[allow(clippy::ptr_arg)]
    fn rebuild_subagent_tree(&self, messages: &mut Vec<TranscriptMessage>) {
        // Collect indexes of subagent status rows (not the header).
        let subagent_indexes: Vec<usize> = messages
            .iter()
            .enumerate()
            .filter(|(_, m)| {
                m.startup_key
                    .as_deref()
                    .is_some_and(|k| k.starts_with("subagent:") && k != "subagent:header")
            })
            .map(|(i, _)| i)
            .collect();

        let count = subagent_indexes.len();
        for (pos, &idx) in subagent_indexes.iter().enumerate() {
            let tree_prefix = if pos == 0 {
                // First child: tree branch connector; last may override below
                "├─"
            } else {
                "├─"
            };
            // Last child gets the corner connector.
            let tree_prefix = if pos == count.saturating_sub(1) {
                "└─"
            } else {
                tree_prefix
            };
            if let Some(msg) = messages.get_mut(idx) {
                msg.tree_prefix = Some(tree_prefix.to_string());
            }
        }
    }

    fn append_assistant(&mut self, messages: &mut Vec<TranscriptMessage>, delta: &str) -> bool {
        if delta.is_empty() {
            return false;
        }
        // Strip protocol tags (<proposed_plan> / </proposed_plan>) from display text.
        let cleaned = strip_plan_tags(delta);
        if cleaned.is_empty() {
            // Tag-only delta — swallow entirely (no visible text).
            return false;
        }
        if let Some(last) = messages.last_mut()
            && last.style == TranscriptStyle::Assistant
        {
            last.content.push_str(&cleaned);
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
        user_shell: bool,
    ) -> bool {
        self.fold_user_shell(messages);
        self.finalize_thinking(messages);
        self.finalize_assistant(messages);
        self.finalize_meta(messages);
        let index = messages.len();
        self.live_tool_indexes.insert(id.clone(), index);
        self.tool_started_at.insert(id, Instant::now());
        let mut msg = TranscriptMessage::tool_call(name, args_summary, TranscriptStyle::ToolRunning);
        msg.user_shell = user_shell;
        messages.push(msg);
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
        let target = if let Some(tool) = message.tool.as_mut() {
            &mut tool.output
        } else {
            &mut message.content
        };
        // Cap streaming output so the card does not slow down rendering a multi-MB string.
        let new_len = target.len().saturating_add(output.len());
        if new_len > TOOL_OUTPUT_STREAM_CAP && !target.is_empty() {
            // Keep only the last TOOL_OUTPUT_STREAM_CAP - chunk_len bytes, prefixed with a marker.
            let drop = new_len.saturating_sub(TOOL_OUTPUT_STREAM_CAP).min(target.len());
            let prefix = "\n[...stream output truncated...]\n";
            *target = format!("{prefix}{}", &target[drop..]);
        }
        target.push_str(output);
        true
    }

    fn end_tool(
        &mut self,
        messages: &mut [TranscriptMessage],
        id: &str,
        is_error: bool,
        output: &str,
        details: &serde_json::Value,
    ) -> bool {
        if let Some(index) = self.live_tool_indexes.remove(id)
            && let Some(message) = messages.get_mut(index)
        {
            if let Some(tool) = message.tool.as_mut() {
                if !output.is_empty() {
                    tool.output = output.to_string();
                }
                // Install edit_file before/after text for the embedded DiffView (if present).
                let _ = tool.apply_tool_result_details(details);
            }
            message.style = if is_error {
                TranscriptStyle::ToolFailed
            } else {
                TranscriptStyle::ToolSuccess
            };
            // Prefer wall-clock from live Instant; fall back to persisted `_elph_ui.duration_secs`.
            if let Some(started) = self.tool_started_at.remove(id) {
                message.duration_secs = Some(format_elapsed_secs(started));
            } else if let Some(secs) = crate::tui::transcript::duration_from_tool_details(details) {
                message.duration_secs = Some(secs);
            }
            // Collapse finished tools for a compact log — user shell stays expanded so
            // the full output remains visible until a new response replaces it.
            // Respect user_pinned: if the user manually toggled the card, leave it alone.
            if !message.user_shell && !message.user_pinned {
                message.detail_expanded = false;
            }
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
                user_shell: false,
            },
        );
        assert_eq!(messages[0].style, TranscriptStyle::ToolRunning);
        applier.apply(
            &mut messages,
            AgentUiEvent::ToolEnd {
                id: "t1".into(),
                is_error: false,
                output: String::new(),
                details: serde_json::json!({}),
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
                user_shell: false,
            },
        );
        applier.apply(
            &mut messages,
            AgentUiEvent::ToolEnd {
                id: "t2".into(),
                is_error: true,
                output: "exit 1".into(),
                details: serde_json::json!({}),
            },
        );
        assert_eq!(messages[0].style, TranscriptStyle::ToolFailed);
        assert_eq!(messages[0].tool.as_ref().unwrap().output, "exit 1");
    }

    #[test]
    fn edit_file_tool_end_stores_diff_payload() {
        let mut messages = Vec::new();
        let mut applier = TranscriptEventApplier::new(false, false);
        applier.apply(
            &mut messages,
            AgentUiEvent::ToolStart {
                id: "edit1".into(),
                name: "edit_file".into(),
                args_summary: r#"{"path":"src/a.rs"}"#.into(),
                user_shell: false,
            },
        );
        applier.apply(
            &mut messages,
            AgentUiEvent::ToolEnd {
                id: "edit1".into(),
                is_error: false,
                output: "Edited src/a.rs".into(),
                details: serde_json::json!({
                    "old_content": "fn a() {}\n",
                    "new_content": "fn a() { 1 }\n",
                    "file_path": "/tmp/src/a.rs",
                }),
            },
        );
        let tool = messages[0].tool.as_ref().expect("tool");
        assert_eq!(tool.old_text.as_deref(), Some("fn a() {}\n"));
        assert_eq!(tool.new_text.as_deref(), Some("fn a() { 1 }\n"));
        assert_eq!(tool.file_path.as_deref(), Some("/tmp/src/a.rs"));
        // edit_file with diff collapses like all other finished tools.
        assert!(!messages[0].detail_expanded);
        assert!(tool.has_inline_diff());
        assert!(messages[0].is_tool_collapsed());
        assert!(messages[0].layout_text().starts_with("✓ Edit "));
        assert!(tool.inline_diff_body_rows() > 0, "diff data is preserved");
    }

    #[test]
    fn read_file_tool_end_stays_collapsed() {
        let mut messages = Vec::new();
        let mut applier = TranscriptEventApplier::new(false, false);
        applier.apply(
            &mut messages,
            AgentUiEvent::ToolStart {
                id: "r1".into(),
                name: "read_file".into(),
                args_summary: r#"{"path":"a.rs"}"#.into(),
                user_shell: false,
            },
        );
        applier.apply(
            &mut messages,
            AgentUiEvent::ToolEnd {
                id: "r1".into(),
                is_error: false,
                output: "fn a() {}".into(),
                details: serde_json::json!({}),
            },
        );
        assert!(!messages[0].detail_expanded);
        assert!(messages[0].is_tool_collapsed());
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
                user_shell: false,
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
    fn subagent_status_upserts_process_row_with_tree_format() {
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
                model: "claude-sonnet-4".into(),
            },
        ));
        // First message is the header, second is the subagent row.
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].startup_key.as_deref(), Some("subagent:header"));
        assert!(messages[0].content.contains("Subagents"));
        assert_eq!(messages[1].style, TranscriptStyle::StatusRunning);
        assert_eq!(messages[1].startup_key.as_deref(), Some("subagent:agent_01"));
        // Content is name-only (model stored separately in model_tag).
        assert_eq!(messages[1].content, "worker-1");
        assert_eq!(messages[1].model_tag.as_deref(), Some("claude-sonnet-4"));
        assert_eq!(messages[1].agent_tag.as_deref(), Some("agent_01"));
        // Tree prefix set (only one agent → └─).
        assert_eq!(messages[1].tree_prefix.as_deref(), Some("└─"));
        // status_detail is None in tree mode.
        assert_eq!(messages[1].status_detail, None);

        // Same agent tool update upserts in place (low noise).
        assert!(applier.apply(
            &mut messages,
            AgentUiEvent::SubagentStatus {
                agent_id: "agent_01".into(),
                agent_path: "main/worker-1".into(),
                task_name: "worker-1".into(),
                phase: SubagentUiPhase::Running,
                message: "tool:shell_exec".into(),
                model: "claude-sonnet-4".into(),
            },
        ));
        assert_eq!(messages.len(), 2); // still header + row
        assert_eq!(messages[1].status_detail, None);

        assert!(applier.apply(
            &mut messages,
            AgentUiEvent::SubagentStatus {
                agent_id: "agent_01".into(),
                agent_path: "main/worker-1".into(),
                task_name: "worker-1".into(),
                phase: SubagentUiPhase::Done,
                message: String::new(),
                model: "claude-sonnet-4".into(),
            },
        ));
        assert_eq!(messages.len(), 2); // header + done row
        assert_eq!(messages[1].style, TranscriptStyle::StatusSuccess);
        assert_eq!(messages[1].status_detail, None);
        assert!(messages[1].duration_secs.is_some());
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
                model: "claude-sonnet-4".into(),
            },
        );
        // First message is the header.
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].startup_key.as_deref(), Some("subagent:header"));
        // Nesting pads the whole row (glyph+label); the title itself stays flush for tight glyph gap.
        assert!(!messages[1].content.starts_with(' '), "{}", messages[1].content);
        assert_eq!(messages[1].status_indent, 2); // depth 2 → one nest past main children
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
                user_shell: false,
            },
        );
        std::thread::sleep(std::time::Duration::from_millis(120));
        applier.apply(
            &mut messages,
            AgentUiEvent::ToolEnd {
                id: "t-dur".into(),
                is_error: false,
                output: String::new(),
                details: serde_json::json!({}),
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
    fn prompt_queue_view_badge_and_replace() {
        let mut queue = PromptQueueView::default();
        assert!(queue.is_empty());
        assert!(queue.badge_label().is_none());
        queue.replace(vec![
            QueuedPromptItem {
                seq: 1,
                kind: QueuedPromptKind::FollowUp,
                kind_index: 0,
                text: "first".into(),
            },
            QueuedPromptItem {
                seq: 2,
                kind: QueuedPromptKind::Steer,
                kind_index: 0,
                text: "second".into(),
            },
        ]);
        assert_eq!(queue.len(), 2);
        assert_eq!(queue.badge_label().as_deref(), Some("Q:2"));
        assert_eq!(queue.items().first().map(|i| i.text.as_str()), Some("first"));
        queue.clear();
        assert!(queue.is_empty());
    }

    #[test]
    fn prompt_queue_send_hides_until_harness_drains() {
        let mut queue = PromptQueueView::default();
        queue.replace(vec![
            QueuedPromptItem {
                seq: 1,
                kind: QueuedPromptKind::FollowUp,
                kind_index: 0,
                text: "do the thing".into(),
            },
            QueuedPromptItem {
                seq: 2,
                kind: QueuedPromptKind::FollowUp,
                kind_index: 1,
                text: "other".into(),
            },
        ]);
        let sent = queue.remove_at_local(0).expect("item");
        queue.suppress_sent(sent.text.clone());
        assert_eq!(queue.len(), 1);
        assert_eq!(queue.items()[0].text, "other");

        // Interject re-queues as steer — must stay hidden.
        queue.replace(vec![
            QueuedPromptItem {
                seq: 1,
                kind: QueuedPromptKind::FollowUp,
                kind_index: 0,
                text: "other".into(),
            },
            QueuedPromptItem {
                seq: 2,
                kind: QueuedPromptKind::Steer,
                kind_index: 0,
                text: "do the thing".into(),
            },
        ]);
        assert_eq!(queue.len(), 1);
        assert_eq!(queue.items()[0].text, "other");

        // After drain, suppress entry is pruned.
        queue.replace(vec![QueuedPromptItem {
            seq: 1,
            kind: QueuedPromptKind::FollowUp,
            kind_index: 0,
            text: "other".into(),
        }]);
        assert_eq!(queue.len(), 1);

        // Same text re-queued later is visible again.
        queue.push_follow_up_local("do the thing".into());
        assert_eq!(queue.len(), 2);
    }

    #[test]
    fn coalesce_merges_adjacent_stream_deltas() {
        let events = vec![
            AgentUiEvent::TextDelta("Hel".into()),
            AgentUiEvent::TextDelta("lo".into()),
            AgentUiEvent::ToolStart {
                id: "t".into(),
                name: "read_file".into(),
                args_summary: "{}".into(),
                user_shell: false,
            },
            AgentUiEvent::ToolUpdate {
                id: "t".into(),
                output: "a".into(),
            },
            AgentUiEvent::ToolUpdate {
                id: "t".into(),
                output: "b".into(),
            },
            AgentUiEvent::ThinkingDelta("x".into()),
            AgentUiEvent::ThinkingDelta("y".into()),
        ];
        let coalesced = coalesce_agent_ui_events(events);
        assert_eq!(coalesced.len(), 4);
        match &coalesced[0] {
            AgentUiEvent::TextDelta(s) => assert_eq!(s, "Hello"),
            other => panic!("expected TextDelta, got {other:?}"),
        }
        match &coalesced[2] {
            AgentUiEvent::ToolUpdate { output, .. } => assert_eq!(output, "ab"),
            other => panic!("expected ToolUpdate, got {other:?}"),
        }
        match &coalesced[3] {
            AgentUiEvent::ThinkingDelta(s) => assert_eq!(s, "xy"),
            other => panic!("expected ThinkingDelta, got {other:?}"),
        }
    }
}
