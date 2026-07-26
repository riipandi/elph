//! Auto session title generation via LLM.

use elph_ai::{Context, Message, Models, SimpleStreamOptions, StopReason};

use super::builtin::session_name::SESSION_NAME_SYSTEM_PROMPT;
use super::builtin::session_name::{build_session_name_prompt, extract_conversation_for_naming, sanitize_session_name};
use crate::types::AgentMessage;

/// Generate a short session title from the conversation transcript.
///
/// Uses the built-in system/user prompts. Prefer
/// [`generate_session_name_with_prompts`] when the host supplies template files.
///
/// Returns `None` when there is no naming-worthy content or the model call fails.
pub async fn generate_session_name(
    messages: &[AgentMessage],
    models: &Models,
    model: &elph_ai::Model,
) -> Option<String> {
    let conversation = extract_conversation_for_naming(messages);
    if conversation.trim().is_empty() {
        return None;
    }
    let user_prompt = build_session_name_prompt(&conversation);
    generate_session_name_with_prompts(messages, models, model, SESSION_NAME_SYSTEM_PROMPT, &user_prompt).await
}

/// Generate a session title using host-supplied system and user prompts.
///
/// `user_prompt` should already include the conversation excerpt (or be a fully
/// rendered template). `messages` is still used to decide whether content is
/// naming-worthy via [`extract_conversation_for_naming`].
pub async fn generate_session_name_with_prompts(
    messages: &[AgentMessage],
    models: &Models,
    model: &elph_ai::Model,
    system_prompt: &str,
    user_prompt: &str,
) -> Option<String> {
    let conversation = extract_conversation_for_naming(messages);
    if conversation.trim().is_empty() {
        return None;
    }
    let user_prompt = user_prompt.trim();
    if user_prompt.is_empty() {
        return None;
    }

    let response = models
        .complete_simple(
            model,
            &Context {
                system_prompt: Some(system_prompt.trim().to_string()),
                messages: vec![Message::User {
                    content: elph_ai::UserContent::Text(user_prompt.to_string()),
                    timestamp: now_millis(),
                }],
                tools: None,
            },
            Some({
                let mut options = SimpleStreamOptions::from_stream(elph_ai::StreamOptions::default());
                options.base.max_tokens = Some(64);
                options
            }),
        )
        .await;

    if !matches!(response.stop_reason, StopReason::Stop | StopReason::Length) {
        return None;
    }

    let text = response
        .content
        .iter()
        .filter_map(|block| match block {
            elph_ai::AssistantContentBlock::Text(text) => Some(text.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");
    let title = sanitize_session_name(&text);
    if title.is_empty() { None } else { Some(title) }
}

fn now_millis() -> i64 {
    (time::OffsetDateTime::now_utc().unix_timestamp_nanos() / 1_000_000) as i64
}
