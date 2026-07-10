//! LLM summarization for compaction.

use elph_ai::{AssistantContentBlock, Context, Message, Model, Models, SimpleStreamOptions, StopReason, ThinkingLevel};
use tokio_util::sync::CancellationToken;

use crate::compaction::utils::serialize_conversation;
use crate::harness::types::{CompactionError, CompactionErrorCode};

use crate::messages::default_convert_to_llm;
use crate::types::AgentMessage;

use super::types::{
    SUMMARIZATION_PROMPT, SUMMARIZATION_SYSTEM_PROMPT, TURN_PREFIX_SUMMARIZATION_PROMPT, UPDATE_SUMMARIZATION_PROMPT,
};

fn now_millis() -> i64 {
    (time::OffsetDateTime::now_utc().unix_timestamp_nanos() / 1_000_000) as i64
}

fn assistant_text_content(message: &elph_ai::AssistantMessage) -> String {
    message
        .content
        .iter()
        .filter_map(|block| match block {
            AssistantContentBlock::Text(text) => Some(text.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn build_stream_options(
    model: &Model,
    max_tokens: u64,
    signal: Option<CancellationToken>,
    thinking_level: Option<ThinkingLevel>,
) -> SimpleStreamOptions {
    let mut options = SimpleStreamOptions::from_stream(elph_ai::StreamOptions::default());
    options.base.max_tokens = Some(max_tokens as u32);
    options.base.signal = signal;
    if model.reasoning
        && let Some(level) = thinking_level
    {
        options.reasoning = Some(level);
    }
    options
}

/// Generate or update a conversation summary for compaction.
#[allow(clippy::too_many_arguments)]
pub async fn generate_summary(
    current_messages: &[AgentMessage],
    models: &Models,
    model: &Model,
    reserve_tokens: u64,
    signal: Option<CancellationToken>,
    custom_instructions: Option<&str>,
    previous_summary: Option<&str>,
    thinking_level: Option<ThinkingLevel>,
) -> std::result::Result<String, CompactionError> {
    let max_tokens = std::cmp::min(
        (reserve_tokens as f64 * 0.8).floor() as u64,
        if model.max_tokens > 0 {
            model.max_tokens as u64
        } else {
            u64::MAX
        },
    );

    let base_prompt = if previous_summary.is_some() {
        UPDATE_SUMMARIZATION_PROMPT.to_string()
    } else {
        SUMMARIZATION_PROMPT.to_string()
    };
    let base_prompt = if let Some(instructions) = custom_instructions {
        format!("{base_prompt}\n\nAdditional focus: {instructions}")
    } else {
        base_prompt
    };

    let llm_messages = default_convert_to_llm(current_messages.to_vec());
    let conversation_text = serialize_conversation(&llm_messages);
    let mut prompt_text = format!("<conversation>\n{conversation_text}\n</conversation>\n\n");
    if let Some(summary) = previous_summary {
        prompt_text.push_str(&format!("<previous-summary>\n{summary}\n</previous-summary>\n\n"));
    }
    prompt_text.push_str(&base_prompt);

    let summarization_messages = vec![Message::User {
        content: elph_ai::UserContent::Text(prompt_text),
        timestamp: now_millis(),
    }];

    let response = models
        .complete_simple(
            model,
            &Context {
                system_prompt: Some(SUMMARIZATION_SYSTEM_PROMPT.to_string()),
                messages: summarization_messages,
                tools: None,
            },
            Some(build_stream_options(model, max_tokens, signal, thinking_level)),
        )
        .await;

    match response.stop_reason {
        StopReason::Aborted => Err(CompactionError::new(
            CompactionErrorCode::Aborted,
            response
                .error_message
                .unwrap_or_else(|| "Summarization aborted".to_string()),
        )),
        StopReason::Error => Err(CompactionError::new(
            CompactionErrorCode::SummarizationFailed,
            format!(
                "Summarization failed: {}",
                response.error_message.unwrap_or_else(|| "Unknown error".to_string())
            ),
        )),
        _ => Ok(assistant_text_content(&response)),
    }
}

pub(super) async fn generate_turn_prefix_summary(
    messages: &[AgentMessage],
    models: &Models,
    model: &Model,
    reserve_tokens: u64,
    signal: Option<CancellationToken>,
    thinking_level: Option<ThinkingLevel>,
) -> std::result::Result<String, CompactionError> {
    let max_tokens = std::cmp::min(
        (reserve_tokens as f64 * 0.5).floor() as u64,
        if model.max_tokens > 0 {
            model.max_tokens as u64
        } else {
            u64::MAX
        },
    );
    let llm_messages = default_convert_to_llm(messages.to_vec());
    let conversation_text = serialize_conversation(&llm_messages);
    let prompt_text =
        format!("<conversation>\n{conversation_text}\n</conversation>\n\n{TURN_PREFIX_SUMMARIZATION_PROMPT}");
    let summarization_messages = vec![Message::User {
        content: elph_ai::UserContent::Text(prompt_text),
        timestamp: now_millis(),
    }];

    let response = models
        .complete_simple(
            model,
            &Context {
                system_prompt: Some(SUMMARIZATION_SYSTEM_PROMPT.to_string()),
                messages: summarization_messages,
                tools: None,
            },
            Some(build_stream_options(model, max_tokens, signal, thinking_level)),
        )
        .await;

    match response.stop_reason {
        StopReason::Aborted => Err(CompactionError::new(
            CompactionErrorCode::Aborted,
            response
                .error_message
                .unwrap_or_else(|| "Turn prefix summarization aborted".to_string()),
        )),
        StopReason::Error => Err(CompactionError::new(
            CompactionErrorCode::SummarizationFailed,
            format!(
                "Turn prefix summarization failed: {}",
                response.error_message.unwrap_or_else(|| "Unknown error".to_string())
            ),
        )),
        _ => Ok(assistant_text_content(&response)),
    }
}
