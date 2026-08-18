//! Session continuity brief for restored / long-running sessions.
//!
//! When a session is resumed (`--continue` / `--resume`) or already has history,
//! the model only sees the raw branch messages unless we also surface structured
//! state (open todos, active goal, last user/assistant anchors). Without that,
//! agents often re-plan and redo completed work.
//!
//! This module builds a compact `<session_state>` block for the system prompt
//! and helpers to rehydrate the TUI todo panel on bootstrap.

use elph_agent::AgentMessage;
use elph_agent::goals::{Goal, GoalStatus};
use elph_agent::session::SessionTreeEntry;
use elph_agent::todos::{TodoItem, TodoStatus};
use elph_ai::{AssistantContentBlock, AssistantMessage, ContentBlock, Message, TextContent, UserContent};

/// Max characters for a single anchored quote in the brief.
const ANCHOR_CHARS: usize = 280;
/// Max open todos listed in the brief (overflow summarized).
const MAX_TODO_LINES: usize = 12;

/// Structured snapshot used to build the continuity brief.
#[derive(Debug, Clone, Default)]
pub struct ContinuitySnapshot {
    pub session_id: String,
    /// True when the branch already has user/assistant turns (resume or mid-session).
    pub has_history: bool,
    pub goal: Option<GoalBrief>,
    pub todos: Vec<TodoBrief>,
    pub last_user: Option<String>,
    pub last_assistant: Option<String>,
    pub open_todo_count: usize,
    pub done_todo_count: usize,
    pub total_todo_count: usize,
}

#[derive(Debug, Clone)]
pub struct GoalBrief {
    pub objective: String,
    pub status: String,
}

#[derive(Debug, Clone)]
pub struct TodoBrief {
    pub content: String,
    pub status: TodoStatus,
}

impl ContinuitySnapshot {
    pub fn from_parts(
        session_id: impl Into<String>,
        branch: &[SessionTreeEntry],
        todos: &[TodoItem],
        goal: Option<&Goal>,
    ) -> Self {
        let (last_user, last_assistant) = last_message_anchors(branch);
        let has_history = branch.iter().any(|e| {
            matches!(
                e,
                SessionTreeEntry::Message {
                    message: AgentMessage::Llm(_),
                    ..
                }
            )
        });
        let open_todo_count = todos
            .iter()
            .filter(|t| matches!(t.status, TodoStatus::Pending | TodoStatus::InProgress))
            .count();
        let done_todo_count = todos
            .iter()
            .filter(|t| matches!(t.status, TodoStatus::Completed | TodoStatus::Cancelled))
            .count();
        let todo_briefs: Vec<TodoBrief> = todos
            .iter()
            .filter(|t| matches!(t.status, TodoStatus::Pending | TodoStatus::InProgress))
            .take(MAX_TODO_LINES)
            .map(|t| TodoBrief {
                content: t.content.clone(),
                status: t.status,
            })
            .collect();
        let goal_brief = goal.map(|g| GoalBrief {
            objective: g.objective.clone(),
            status: g.status.as_str().to_string(),
        });
        Self {
            session_id: session_id.into(),
            has_history,
            goal: goal_brief,
            todos: todo_briefs,
            last_user,
            last_assistant,
            open_todo_count,
            done_todo_count,
            total_todo_count: todos.len(),
        }
    }

    /// Whether the brief has anything useful beyond empty defaults.
    pub fn is_meaningful(&self) -> bool {
        self.has_history
            || self.goal.is_some()
            || self.total_todo_count > 0
            || self.last_user.is_some()
            || self.last_assistant.is_some()
    }

    /// Compact XML-ish block for system prompt injection.
    pub fn render(&self) -> String {
        if !self.is_meaningful() {
            return String::new();
        }

        let mut out = String::from("<session_state>\n");
        if !self.session_id.is_empty() {
            out.push_str(&format!("session_id: {}\n", self.session_id));
        }
        out.push_str(
            "Continuing an existing session. Prior branch messages are already in context — \
do not restart finished work, re-map the repo, or re-run completed tool steps unless the user asks. \
Read anchors + open todos/goal below; prefer status merges on existing todos over a new plan.\n",
        );

        if let Some(ref g) = self.goal {
            // Skip completed goals that no longer steer the session.
            if g.status != GoalStatus::Complete.as_str() {
                out.push_str(&format!("goal: [{}] {}\n", g.status, truncate_chars(&g.objective, 200)));
            }
        }

        if self.total_todo_count > 0 {
            out.push_str(&format!(
                "todos: {} open / {} done / {} total\n",
                self.open_todo_count, self.done_todo_count, self.total_todo_count
            ));
            for t in &self.todos {
                let mark = match t.status {
                    TodoStatus::InProgress => "in_progress",
                    TodoStatus::Pending => "pending",
                    TodoStatus::Completed => "completed",
                    TodoStatus::Cancelled => "cancelled",
                };
                out.push_str(&format!("- [{mark}] {}\n", truncate_chars(&t.content, 160)));
            }
            if self.open_todo_count > self.todos.len() {
                out.push_str(&format!(
                    "- … +{} more open (use todo_read)\n",
                    self.open_todo_count.saturating_sub(self.todos.len())
                ));
            }
        }

        if let Some(ref u) = self.last_user {
            out.push_str(&format!("last_user: {}\n", truncate_chars(u, ANCHOR_CHARS)));
        }
        if let Some(ref a) = self.last_assistant {
            out.push_str(&format!("last_assistant: {}\n", truncate_chars(a, ANCHOR_CHARS)));
        }

        out.push_str("</session_state>");
        out
    }
}

fn last_message_anchors(branch: &[SessionTreeEntry]) -> (Option<String>, Option<String>) {
    let mut last_user = None;
    let mut last_assistant = None;
    for entry in branch {
        let SessionTreeEntry::Message { message, .. } = entry else {
            continue;
        };
        let Some(llm) = message.as_llm() else {
            continue;
        };
        match llm {
            Message::User { content, .. } => {
                if let Some(text) = user_text(content) {
                    let t = text.trim();
                    if !t.is_empty() {
                        last_user = Some(t.to_string());
                    }
                }
            }
            Message::Assistant(asst) => {
                let text = assistant_text(asst);
                let t = text.trim();
                if !t.is_empty() {
                    last_assistant = Some(t.to_string());
                }
            }
            Message::ToolResult { .. } => {}
        }
    }
    (last_user, last_assistant)
}

fn user_text(content: &UserContent) -> Option<String> {
    match content {
        UserContent::Text(t) => Some(t.clone()),
        UserContent::Blocks(blocks) => {
            let mut s = String::new();
            for b in blocks {
                if let ContentBlock::Text { text } = b {
                    if !s.is_empty() {
                        s.push(' ');
                    }
                    s.push_str(text);
                }
            }
            if s.is_empty() { None } else { Some(s) }
        }
    }
}

fn assistant_text(asst: &AssistantMessage) -> String {
    let mut s = String::new();
    for block in &asst.content {
        if let AssistantContentBlock::Text(TextContent { text, .. }) = block {
            if !s.is_empty() {
                s.push(' ');
            }
            s.push_str(text);
        }
    }
    s
}

fn truncate_chars(s: &str, max: usize) -> String {
    let count = s.chars().count();
    if count <= max {
        return s.replace(['\r', '\n'], " ");
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out.replace(['\r', '\n'], " ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use elph_agent::todos::TodoStatus;

    #[test]
    fn empty_snapshot_renders_empty() {
        let s = ContinuitySnapshot::default();
        assert!(!s.is_meaningful());
        assert!(s.render().is_empty());
    }

    #[test]
    fn renders_open_todos_and_rule() {
        let s = ContinuitySnapshot {
            session_id: "s1".into(),
            has_history: true,
            goal: Some(GoalBrief {
                objective: "Ship todo panel".into(),
                status: "active".into(),
            }),
            todos: vec![TodoBrief {
                content: "Wire restore".into(),
                status: TodoStatus::InProgress,
            }],
            last_user: Some("continue the panel work".into()),
            last_assistant: Some("Next I will wire session continuity".into()),
            open_todo_count: 1,
            done_todo_count: 2,
            total_todo_count: 3,
        };
        let text = s.render();
        assert!(text.contains("<session_state>"));
        assert!(text.contains("do not restart finished work"));
        assert!(text.contains("[in_progress] Wire restore"));
        assert!(text.contains("last_user: continue the panel work"));
        assert!(text.contains("goal: [active] Ship todo panel"));
    }
}
