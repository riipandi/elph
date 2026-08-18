//! Post-turn todo hardening: close stale todos after a successful turn.
//!
//! Some models finish their work but never call `todo_write` to mark items
//! completed — or their `completed` write is rejected because the work tracker
//! cannot prove the work per item. This hook is the backstop: it runs on every
//! `TurnEnd`, and when the final assistant answer carries a completion signal
//! it closes the todos the turn provably finished.
//!
//! Guards against premature closure:
//! - Only text-only final answers are considered (tool-call cycles and
//!   errored/aborted/truncated turns never auto-close).
//! - Completion requires a completion signal in the final text and no
//!   continuation marker ("not done", "continuing", …).
//! - Work proof comes from the `WorkTracker` (work since the item entered the
//!   plan) or from mutating tool calls since the previous text-only answer.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::Result;
use elph_agent::harness::{AgentHarness, AgentHarnessEvent};
use elph_agent::{
    AgentEvent, AgentMessage, TodoHook, TodoStore, TursoSessionStorage, WorkTracker, auto_close_done_todos,
};
use elph_ai::{AssistantContentBlock, Message, StopReason};

/// Continuation markers — signals that the turn is *not* finished. Checked
/// first so short negatives like "not done" never count as completion.
const CONTINUATION_MARKERS: &[&str] = &[
    "let me continue",
    "i'll continue",
    "i will continue",
    "i'll keep",
    "i will keep",
    "continuing",
    "to be continued",
    "not done",
    "not finished",
    "not complete",
    "incomplete",
    "unfinished",
    "still working",
    "still need",
    "still have",
    "one more",
    "next step",
    "next, i",
    "next i will",
    "next i'll",
    "then i'll",
    "then i will",
    "i'll now",
    "i will now",
    "waiting for",
    "let me know",
    "now running",
    "now testing",
    "now let me",
    "let me run",
    "let me test",
    "i'll run",
    "i will run",
    "i'll test",
    "i will test",
    "testing",
    "running tests",
    "running the tests",
];

/// Completion markers — text that wraps up a finished turn.
const COMPLETION_MARKERS: &[&str] = &[
    "done",
    "finished",
    "complete",
    "completed",
    "resolved",
    "all done",
    "task complete",
    "everything is done",
    "work is done",
    "it's done",
    "its done",
    "that's it",
    "thats it",
    "no more tasks",
    "no further tasks",
    "all tasks",
    "ready for review",
];

/// Register the post-turn todo auto-close hook.
pub async fn register_todo_auto_close_hook(
    harness: &AgentHarness<TursoSessionStorage>,
    store: Arc<TodoStore>,
    session_id: String,
    work_tracker: Arc<WorkTracker>,
    on_update: TodoHook,
) -> Result<()> {
    // Work counter recorded at the last text-only TurnEnd. The gap between it
    // and now is the mutating work attributable to the current final answer.
    let last_text_end_work = Arc::new(AtomicU64::new(work_tracker.current()));

    harness
        .subscribe({
            let store = Arc::clone(&store);
            let session_id = session_id.clone();
            let work_tracker = Arc::clone(&work_tracker);
            let on_update = Arc::clone(&on_update);
            let last_text_end_work = Arc::clone(&last_text_end_work);
            move |event, _| {
                let store = Arc::clone(&store);
                let session_id = session_id.clone();
                let work_tracker = Arc::clone(&work_tracker);
                let on_update = Arc::clone(&on_update);
                let last_text_end_work = Arc::clone(&last_text_end_work);
                Box::pin(async move {
                    let agent_event = match event {
                        AgentHarnessEvent::Agent(e) => e,
                        _ => return,
                    };
                    let message = match agent_event {
                        AgentEvent::TurnEnd { message, .. } => message,
                        _ => return,
                    };
                    if !turn_is_final_answer(&message) {
                        return;
                    }
                    let work_before = last_text_end_work.swap(work_tracker.current(), Ordering::Relaxed);
                    let turn_did_mutating_work = work_tracker.current() > work_before;
                    if !has_completion_signal(&assistant_text(&message)) {
                        return;
                    }
                    match auto_close_done_todos(&store, &session_id, &work_tracker, turn_did_mutating_work).await {
                        Ok(items) => on_update(items).await,
                        Err(err) => log::warn!("todo auto-close: {err:#}"),
                    }
                })
            }
        })
        .await;
    Ok(())
}

/// Whether this TurnEnd message is a final, text-only answer.
///
/// Tool-call cycles (the model is still working) and errored/aborted/truncated
/// turns never count as final — a `Length`-stopped message is cut mid-sentence
/// and must not auto-close anything.
fn turn_is_final_answer(message: &AgentMessage) -> bool {
    let Some(Message::Assistant(assistant)) = message.as_llm() else {
        return false;
    };
    if assistant.error_message.is_some()
        || matches!(
            assistant.stop_reason,
            StopReason::Error | StopReason::Aborted | StopReason::Length
        )
    {
        return false;
    }
    !assistant
        .content
        .iter()
        .any(|block| matches!(block, AssistantContentBlock::ToolCall(_)))
}

/// Concatenated text blocks of the final assistant message.
fn assistant_text(message: &AgentMessage) -> String {
    let Some(Message::Assistant(assistant)) = message.as_llm() else {
        return String::new();
    };
    assistant
        .content
        .iter()
        .filter_map(|block| match block {
            AssistantContentBlock::Text(t) => Some(t.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// True when the final text signals the turn's work is wrapped up:
/// a completion marker and no continuation marker.
pub(crate) fn has_completion_signal(text: &str) -> bool {
    let lower = text.to_lowercase();
    if CONTINUATION_MARKERS.iter().any(|m| lower.contains(m)) {
        return false;
    }
    COMPLETION_MARKERS.iter().any(|m| lower.contains(m))
}

#[cfg(test)]
mod tests {
    use super::*;
    use elph_agent::llm_message_to_agent;
    use elph_ai::{TextContent, ToolCall};

    fn final_message(text: &str) -> AgentMessage {
        let assistant =
            elph_ai::api::faux::faux_assistant_message(vec![AssistantContentBlock::Text(TextContent::new(text))], None);
        llm_message_to_agent(Message::Assistant(assistant))
    }

    fn tool_call_cycle_message(text: &str) -> AgentMessage {
        let assistant = elph_ai::api::faux::faux_assistant_message(
            vec![
                AssistantContentBlock::Text(TextContent::new(text)),
                AssistantContentBlock::ToolCall(ToolCall {
                    kind: "toolUse".into(),
                    id: "call_1".into(),
                    name: "read_file".into(),
                    arguments: serde_json::json!({}),
                    thought_signature: None,
                }),
            ],
            Some(StopReason::ToolUse),
        );
        llm_message_to_agent(Message::Assistant(assistant))
    }

    fn error_message() -> AgentMessage {
        let mut assistant = elph_ai::api::faux::faux_assistant_message(vec![], Some(StopReason::Error));
        assistant.error_message = Some("upstream stream failure".into());
        llm_message_to_agent(Message::Assistant(assistant))
    }

    fn truncated_message() -> AgentMessage {
        let assistant = elph_ai::api::faux::faux_assistant_message(
            vec![AssistantContentBlock::Text(TextContent::new(
                "All done — but this message was cut off by the token limit",
            ))],
            Some(StopReason::Length),
        );
        llm_message_to_agent(Message::Assistant(assistant))
    }

    #[test]
    fn final_text_only_answer_is_final() {
        assert!(turn_is_final_answer(&final_message("Done, all items are finished.")));
        assert!(!turn_is_final_answer(&tool_call_cycle_message("Let me check the file.")));
        assert!(!turn_is_final_answer(&error_message()));
        assert!(!turn_is_final_answer(&truncated_message()));
    }

    #[test]
    fn english_completion_signals() {
        assert!(has_completion_signal("All done, todos are updated."));
        assert!(has_completion_signal("The task is complete."));
        assert!(has_completion_signal("I finished everything."));
        assert!(has_completion_signal("CLI refactor complete."));
    }

    #[test]
    fn continuation_signals_block_completion() {
        assert!(!has_completion_signal("Task 1 done, continuing with task 2."));
        assert!(!has_completion_signal("I'm not done yet; next step requires review."));
        assert!(!has_completion_signal("The change is incomplete."));
        assert!(!has_completion_signal("Let me know if you want me to continue."));
        assert!(!has_completion_signal("Incomplete — there is still more to check."));
    }

    #[test]
    fn mid_work_answers_do_not_count_as_completion() {
        assert!(!has_completion_signal("Done with edits, now running tests."));
        assert!(!has_completion_signal("Finished the first stage; testing now."));
    }
}
