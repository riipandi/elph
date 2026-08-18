//! Parallel inbound worker-message answer (does not take `turn_gate`).
//!
//! Snapshots session context, runs a short tool loop with intercom-only tools
//! (`worker_reply`, `worker_list`, `worker_pending`), and writes the mailbox
//! response so the peer's `worker_ask` unblocks while the user's turn continues.

use std::sync::Arc;

use elph_agent::{AgentToolResult, LocalExecutionEnv, ToolContext, ToolResultContent};
use elph_ai::{
    AssistantContentBlock, ContentBlock, Context, Message, SimpleStreamOptions, StopReason, StreamOptions, UserContent,
};

use super::aside::{WORKER_INBOUND_PROMPT_PREFIX, now_millis, snapshot_side_messages};
use super::session::CodingAgentSession;

/// Cap tool/LLM rounds so a stuck model cannot run forever beside the user turn.
const MAX_INTERCOM_STEPS: usize = 6;

pub async fn answer_worker_inbound(
    session: &CodingAgentSession,
    from_worker: &str,
    text: &str,
    msg: &elph_agent::WorkerMessage,
) -> Result<(), String> {
    let Some(rt) = session.worker_runtime.as_ref() else {
        return Err("worker runtime is not available".into());
    };

    let tools = rt.create_intercom_tools();
    let ai_tools: Vec<elph_ai::Tool> = tools.iter().map(|t| t.tool.clone()).collect();

    let mut messages = snapshot_side_messages(session).await?;
    messages.push(Message::User {
        content: UserContent::Text(worker_inbound_instruction(from_worker, text, &msg.id)),
        timestamp: now_millis(),
    });

    let selection = session.selection.read().clone();
    let model = selection.model.clone();
    let models = selection.models.clone();
    let worker_name = session.worker_name();
    let worker_peers = match session.worker_runtime.as_ref() {
        Some(rt) => {
            let s = rt.peer_names_summary().await;
            if s.is_empty() { None } else { Some(s) }
        }
        None => None,
    };
    let system_prompt = Some(
        super::prompt::build_intercom_system_prompt(worker_name, worker_peers.as_deref()).map_err(|e| e.to_string())?,
    );

    let max_tokens = if model.max_tokens > 0 {
        model.max_tokens.clamp(256, 4096)
    } else {
        2048
    };
    let mut options = SimpleStreamOptions::from_stream(StreamOptions::default());
    options.base.max_tokens = Some(max_tokens);

    let dummy_ctx = dummy_tool_context();
    let mut last_text = String::new();
    let mut replied = false;

    for _ in 0..MAX_INTERCOM_STEPS {
        let response = models
            .complete_simple(
                &model,
                &Context {
                    system_prompt: system_prompt.clone(),
                    messages: messages.clone(),
                    tools: Some(ai_tools.clone()),
                },
                Some(options.clone()),
            )
            .await;

        if response.stop_reason == StopReason::Error {
            let detail = response.error_message.clone().unwrap_or_else(|| "model error".into());
            return Err(detail);
        }

        last_text = assistant_text(&response);
        messages.push(Message::Assistant(response.clone()));

        let calls: Vec<_> = response
            .content
            .iter()
            .filter_map(|b| match b {
                AssistantContentBlock::ToolCall(call) => Some(call.clone()),
                _ => None,
            })
            .collect();

        if calls.is_empty() {
            break;
        }

        for call in calls {
            let Some(tool) = tools.iter().find(|t| t.name() == call.name) else {
                messages.push(tool_error_message(&call.id, &call.name, "unknown intercom tool"));
                continue;
            };
            let result = (tool.execute)(call.id.clone(), call.arguments.clone(), None, None, dummy_ctx.clone()).await;
            match result {
                Ok(res) => {
                    if call.name == "worker_reply" {
                        replied = true;
                    }
                    messages.push(tool_result_message(&call.id, &call.name, &res, false));
                }
                Err(err) => {
                    messages.push(tool_error_message(&call.id, &call.name, &err.to_string()));
                }
            }
        }

        if replied {
            break;
        }
    }

    if replied {
        return Ok(());
    }

    if last_text.trim().is_empty() {
        return Err("intercom answer produced no text or worker_reply".into());
    }

    // Model answered in prose — still close the ask so the peer does not time out.
    rt.mailbox()
        .send_response(
            &rt.project_key,
            rt.worker_id.as_str(),
            session.session_id(),
            &msg.from_session_id,
            &msg.id,
            last_text.trim(),
            None,
        )
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn worker_inbound_instruction(from_worker: &str, text: &str, msg_id: &str) -> String {
    format!(
        "{WORKER_INBOUND_PROMPT_PREFIX} (`{from_worker}`)\n\
         in this shared project. The user's main task keeps running — you are a \
         separate answer loop. Reply with the `worker_reply` tool.\n\
         Pass `in_reply_to` = {msg_id} (the message you are answering).\n\
         You may use worker_list / worker_pending. Do not wait for the user turn.\n\
         If the message needs no answer, send a short acknowledgement.</intercom>\n\n\
         {text}"
    )
}

fn dummy_tool_context() -> ToolContext {
    ToolContext::new(Arc::new(LocalExecutionEnv::new(".")))
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

fn tool_result_text(result: &AgentToolResult) -> String {
    result
        .content
        .iter()
        .filter_map(|c| match c {
            ToolResultContent::Text(t) => Some(t.text.as_str()),
            ToolResultContent::Image(_) => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn tool_result_message(id: &str, name: &str, result: &AgentToolResult, is_error: bool) -> Message {
    Message::ToolResult {
        tool_call_id: id.to_string(),
        tool_name: name.to_string(),
        content: vec![ContentBlock::Text {
            text: tool_result_text(result),
        }],
        details: None,
        added_tool_names: None,
        usage: None,
        is_error,
        timestamp: now_millis(),
    }
}

fn tool_error_message(id: &str, name: &str, error: &str) -> Message {
    Message::ToolResult {
        tool_call_id: id.to_string(),
        tool_name: name.to_string(),
        content: vec![ContentBlock::Text {
            text: error.to_string(),
        }],
        details: None,
        added_tool_names: None,
        usage: None,
        is_error: true,
        timestamp: now_millis(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inbound_instruction_pins_msg_id() {
        let t = worker_inbound_instruction("calm-fox", "status?", "msg-1");
        assert!(t.contains("in_reply_to` = msg-1"));
        assert!(t.starts_with(WORKER_INBOUND_PROMPT_PREFIX));
        assert!(t.contains("status?"));
    }
}
