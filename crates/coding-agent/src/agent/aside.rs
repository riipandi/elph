//! `/aside` — side question without interrupting the main agent turn.
//!
//! Snapshots harness state, runs a one-shot tool-free completion, and does
//! **not** append the Q/A to the session message list.

use std::sync::atomic::{AtomicU64, Ordering};

use elph_agent::messages::default_convert_to_llm;
use elph_agent::types::AgentMessage;
use elph_ai::{AssistantContentBlock, Context, Message, SimpleStreamOptions, StopReason, StreamOptions, UserContent};

use super::events::AgentUiEvent;
use super::session::CodingAgentSession;

static ASIDE_REQUEST_SEQ: AtomicU64 = AtomicU64::new(1);

/// Monotonic request id so late/superseded answers can be dropped in the UI.
pub type AsideRequestId = u64;

fn next_aside_request_id() -> AsideRequestId {
    ASIDE_REQUEST_SEQ.fetch_add(1, Ordering::Relaxed)
}

/// Extract the human-readable text from a worker mailbox payload (`{"text": …}`).
/// Falls back to the raw payload when it is not JSON.
pub fn extract_worker_payload_text(payload: &str) -> String {
    match serde_json::from_str::<serde_json::Value>(payload) {
        Ok(v) => v
            .get("text")
            .and_then(|t| t.as_str())
            .map(str::to_string)
            .unwrap_or_else(|| payload.to_string()),
        Err(_) => payload.to_string(),
    }
}

/// Intercom wrapper for the side question (Grok `/btw` semantics).
fn side_question_user_text(question: &str) -> String {
    format!(
        "<intercom>This is a side question from the user. \
         You must answer this question directly in a single response.\n\n\
         IMPORTANT CONTEXT:\n\
         - You are a separate, lightweight agent spawned to answer this one question\n\
         - The main agent is NOT interrupted — it continues working independently in the background\n\
         - You share the conversation context but are a completely separate instance\n\
         - Do NOT reference being interrupted or what you were \"previously doing\" — that framing is incorrect\n\n\
         CRITICAL CONSTRAINTS:\n\
         - Do NOT call any tools; respond with plain text only\n\
         - A tool call cannot help you: nothing runs on the user's machine and you get no turn in which to read a result\n\
         - This is a one-off response — there will be no follow-up turns\n\
         - You can ONLY provide information based on what you already know from the conversation context\n\
         - NEVER say things like \"Let me try...\", \"I'll now...\", \"Let me check...\", or promise to take any action\n\
         - If you don't know the answer, say so — do not offer to look it up or investigate\n\n\
         Simply answer the question with the information you have.</intercom>\n\n\
         {question}"
    )
}

/// Marker on the intercom-loop user instruction (not a harness / transcript turn).
///
/// Older session trees may still contain this prefix; the TUI still maps those
/// rows to a slim meta label.
pub const WORKER_INBOUND_PROMPT_PREFIX: &str = "<intercom>This is a message from another Elph worker";

/// Drop a trailing assistant message that still has tool calls without matching
/// tool results (mid-turn snapshot). Mirrors Grok `pop_trailing_tool_run`.
pub(crate) fn pop_trailing_unpaired_tool_run(messages: &mut Vec<Message>) {
    let Some(last) = messages.last() else {
        return;
    };
    let Message::Assistant(assistant) = last else {
        return;
    };
    let has_tool_call = assistant.content.iter().any(|b| b.is_tool_call());
    if !has_tool_call {
        return;
    }
    messages.pop();
}

fn assistant_text(message: &elph_ai::AssistantMessage) -> String {
    message
        .content
        .iter()
        .filter_map(|block| match block {
            AssistantContentBlock::Text(text) => Some(text.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

/// Run a side question and push UI events. Does **not** take `turn_gate`.
pub async fn run_aside(session: &CodingAgentSession, question: &str, request_id: AsideRequestId) {
    let question = question.trim();
    if question.is_empty() {
        let _ = session.ui_event_sender().send(AgentUiEvent::AsideFailed {
            request_id,
            error: "Usage: /aside <question>".into(),
        });
        return;
    }

    let _ = session.ui_event_sender().send(AgentUiEvent::AsideStarted {
        request_id,
        question: question.to_string(),
    });

    match run_aside_inner(session, question).await {
        Ok(answer) => {
            let _ = session.ui_event_sender().send(AgentUiEvent::AsideFinished {
                request_id,
                question: question.to_string(),
                answer,
            });
        }
        Err(error) => {
            let _ = session
                .ui_event_sender()
                .send(AgentUiEvent::AsideFailed { request_id, error });
        }
    }
}

/// Snapshot session messages for `/aside` and the intercom answer loop.
///
/// Drops a trailing unpaired tool run so a mid-turn snapshot is valid LLM input.
/// The snapshot is not appended back onto the session tree.
pub(crate) async fn snapshot_side_messages(session: &CodingAgentSession) -> Result<Vec<Message>, String> {
    let harness = session.harness();
    let branch = harness
        .session_branch_entries()
        .await
        .map_err(|e| format!("session branch: {e}"))?;
    let session_ctx = elph_agent::session::build_session_context(&branch);
    let agent_messages: Vec<AgentMessage> = session_ctx.messages;
    let mut llm_messages = default_convert_to_llm(agent_messages);
    pop_trailing_unpaired_tool_run(&mut llm_messages);
    Ok(llm_messages)
}

pub(crate) fn now_millis() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

async fn side_completion(
    session: &CodingAgentSession,
    user_instruction: String,
    max_tokens_override: Option<u32>,
) -> Result<String, String> {
    let mut llm_messages = snapshot_side_messages(session).await?;

    llm_messages.push(Message::User {
        content: UserContent::Text(user_instruction),
        timestamp: now_millis(),
    });

    let selection = session.selection.read().clone();
    let model = selection.model.clone();
    let models = selection.models.clone();

    // Best-effort: empty cache stays None (may be slow to compile mid-turn).
    let system_prompt = session.cached_system_prompt().filter(|s| !s.is_empty());

    let max_tokens = max_tokens_override.unwrap_or_else(|| {
        if model.max_tokens > 0 {
            model.max_tokens.clamp(256, 4096)
        } else {
            2048
        }
    });

    let mut options = SimpleStreamOptions::from_stream(StreamOptions::default());
    options.base.max_tokens = Some(max_tokens);

    let response = models
        .complete_simple(
            &model,
            &Context {
                system_prompt,
                messages: llm_messages,
                tools: None,
            },
            Some(options),
        )
        .await;

    if response.stop_reason == StopReason::Error {
        let detail = response.error_message.clone().unwrap_or_else(|| "model error".into());
        return Err(detail);
    }

    let text = assistant_text(&response);
    if text.is_empty() {
        return Err("No response received".into());
    }
    Ok(text)
}

async fn run_aside_inner(session: &CodingAgentSession, question: &str) -> Result<String, String> {
    side_completion(session, side_question_user_text(question), None).await
}

/// Answer `/aside` without reading the session UI event channel.
pub async fn aside_answer(session: &CodingAgentSession, question: &str) -> Result<String, String> {
    run_aside_inner(session, question).await
}

/// Spawn `/aside` work without blocking the UI or the main turn.
pub fn spawn_aside(session: std::sync::Arc<CodingAgentSession>, question: String) -> AsideRequestId {
    let request_id = next_aside_request_id();
    tokio::spawn(async move {
        run_aside(&session, &question, request_id).await;
    });
    request_id
}

#[cfg(test)]
mod tests {
    use super::*;
    use elph_ai::{AssistantMessage, TextContent, ToolCall, Usage};

    fn assistant_with(content: Vec<AssistantContentBlock>, stop: StopReason) -> Message {
        Message::Assistant(AssistantMessage {
            role: "assistant".into(),
            content,
            api: "openai-completions".into(),
            provider: "openai".into(),
            model: "gpt-test".into(),
            response_model: None,
            response_id: None,
            diagnostics: None,
            usage: Usage::default(),
            stop_reason: stop,
            pending_stop_reason: None,
            error_message: None,
            timestamp: 0,
        })
    }

    #[test]
    fn pop_does_nothing_without_tool_calls() {
        let mut msgs = vec![
            Message::User {
                content: UserContent::Text("hi".into()),
                timestamp: 0,
            },
            assistant_with(vec![AssistantContentBlock::Text(TextContent::new("hello"))], StopReason::Stop),
        ];
        let before = msgs.len();
        pop_trailing_unpaired_tool_run(&mut msgs);
        assert_eq!(msgs.len(), before);
    }

    #[test]
    fn pop_removes_trailing_assistant_with_tool_call() {
        let mut msgs = vec![
            Message::User {
                content: UserContent::Text("hi".into()),
                timestamp: 0,
            },
            assistant_with(
                vec![AssistantContentBlock::ToolCall(ToolCall::new(
                    "1",
                    "read_file",
                    serde_json::json!({"path": "x"}),
                ))],
                StopReason::ToolUse,
            ),
        ];
        pop_trailing_unpaired_tool_run(&mut msgs);
        assert_eq!(msgs.len(), 1);
        assert!(matches!(msgs[0], Message::User { .. }));
    }

    #[test]
    fn side_question_text_contains_question() {
        let t = side_question_user_text("what is X?");
        assert!(t.contains("what is X?"));
        assert!(t.contains("side question"));
        assert!(t.contains("Do NOT call any tools"));
    }
}
