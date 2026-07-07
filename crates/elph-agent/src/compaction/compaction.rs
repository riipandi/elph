//! Context compaction logic — ported from pi-agent `harness/compaction/compaction.ts`.

use elph_ai::{
    AssistantContentBlock, Context, Message, Model, Models, SimpleStreamOptions, StopReason, ThinkingLevel, Usage,
};
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::compaction::utils::{
    compute_file_lists, create_file_ops, extract_file_ops_from_message, format_file_operations, serialize_conversation,
};
use crate::harness::types::{CompactionError, CompactionErrorCode, FileOperations};

pub use crate::harness::types::{CompactionPreparation, CompactionSettings};

use crate::messages::default_convert_to_llm;
use crate::messages::{
    CustomMessageBlock, CustomMessageContent, create_branch_summary_message, create_compaction_summary_message,
    create_custom_message,
};
use crate::session::build_session_context;
use crate::session::types::{CustomMessageEntryBlock, CustomMessageEntryContent, SessionTreeEntry};
use crate::types::{AgentMessage, CustomAgentMessage};

/// File-operation details stored on generated compaction entries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompactionDetails {
    pub read_files: Vec<String>,
    pub modified_files: Vec<String>,
}

/// Generated compaction data ready to be persisted as a compaction entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompactionResult {
    pub summary: String,
    pub first_kept_entry_id: String,
    pub tokens_before: u64,
    pub details: Option<CompactionDetails>,
}

pub const SUMMARIZATION_SYSTEM_PROMPT: &str = "You are a context summarization assistant. Your task is to read a conversation between a user and an AI assistant, then produce a structured summary following the exact format specified.

Do NOT continue the conversation. Do NOT respond to any questions in the conversation. ONLY output the structured summary.";

const SUMMARIZATION_PROMPT: &str = "The messages above are a conversation to summarize. Create a structured context checkpoint summary that another LLM will use to continue the work.

Use this EXACT format:

## Goal
[What is the user trying to accomplish? Can be multiple items if the session covers different tasks.]

## Constraints & Preferences
- [Any constraints, preferences, or requirements mentioned by user]
- [Or \"(none)\" if none were mentioned]

## Progress
### Done
- [x] [Completed tasks/changes]

### In Progress
- [ ] [Current work]

### Blocked
- [Issues preventing progress, if any]

## Key Decisions
- **[Decision]**: [Brief rationale]

## Next Steps
1. [Ordered list of what should happen next]

## Critical Context
- [Any data, examples, or references needed to continue]
- [Or \"(none)\" if not applicable]

Keep each section concise. Preserve exact file paths, function names, and error messages.";

const UPDATE_SUMMARIZATION_PROMPT: &str = "The messages above are NEW conversation messages to incorporate into the existing summary provided in <previous-summary> tags.

Update the existing structured summary with new information. RULES:
- PRESERVE all existing information from the previous summary
- ADD new progress, decisions, and context from the new messages
- UPDATE the Progress section: move items from \"In Progress\" to \"Done\" when completed
- UPDATE \"Next Steps\" based on what was accomplished
- PRESERVE exact file paths, function names, and error messages
- If something is no longer relevant, you may remove it

Use this EXACT format:

## Goal
[Preserve existing goals, add new ones if the task expanded]

## Constraints & Preferences
- [Preserve existing, add new ones discovered]

## Progress
### Done
- [x] [Include previously done items AND newly completed items]

### In Progress
- [ ] [Current work - update based on progress]

### Blocked
- [Current blockers - remove if resolved]

## Key Decisions
- **[Decision]**: [Brief rationale] (preserve all previous, add new)

## Next Steps
1. [Update based on current state]

## Critical Context
- [Preserve important context, add new if needed]

Keep each section concise. Preserve exact file paths, function names, and error messages.";

const TURN_PREFIX_SUMMARIZATION_PROMPT: &str =
    "This is the PREFIX of a turn that was too large to keep. The SUFFIX (recent work) is retained.

Summarize the prefix to provide context for the retained suffix:

## Original Request
[What did the user ask for in this turn?]

## Early Progress
- [Key decisions and work done in the prefix]

## Context for Suffix
- [Information needed to understand the retained recent work]

Be concise. Focus on what's needed to understand the kept suffix.";

const ESTIMATED_IMAGE_CHARS: usize = 4800;

fn safe_json_stringify(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "[unserializable]".to_string())
}

/// Calculate total context tokens from provider usage.
pub fn calculate_context_tokens(usage: &Usage) -> u64 {
    if usage.total_tokens > 0 {
        usage.total_tokens
    } else {
        usage.input + usage.output + usage.cache_read + usage.cache_write
    }
}

fn get_assistant_usage(msg: &AgentMessage) -> Option<&Usage> {
    let AgentMessage::Llm(llm) = msg else {
        return None;
    };
    let Message::Assistant(assistant) = llm.as_ref() else {
        return None;
    };
    if matches!(assistant.stop_reason, StopReason::Aborted | StopReason::Error) {
        return None;
    }
    let tokens = calculate_context_tokens(&assistant.usage);
    if tokens > 0 { Some(&assistant.usage) } else { None }
}

/// Return usage from the last valid assistant message in session entries.
pub fn get_last_assistant_usage(entries: &[SessionTreeEntry]) -> Option<Usage> {
    for entry in entries.iter().rev() {
        let SessionTreeEntry::Message { message, .. } = entry else {
            continue;
        };
        if let Some(usage) = get_assistant_usage(message) {
            return Some(usage.clone());
        }
    }
    None
}

/// Estimated context-token usage for a message list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContextUsageEstimate {
    pub tokens: u64,
    pub usage_tokens: u64,
    pub trailing_tokens: u64,
    pub last_usage_index: Option<usize>,
}

fn get_last_assistant_usage_info(messages: &[AgentMessage]) -> Option<(Usage, usize)> {
    for (index, message) in messages.iter().enumerate().rev() {
        if let Some(usage) = get_assistant_usage(message) {
            return Some((usage.clone(), index));
        }
    }
    None
}

/// Estimate context tokens for messages using provider usage when available.
pub fn estimate_context_tokens(messages: &[AgentMessage]) -> ContextUsageEstimate {
    if let Some((usage, index)) = get_last_assistant_usage_info(messages) {
        let usage_tokens = calculate_context_tokens(&usage);
        let trailing_tokens: u64 = messages[index + 1..].iter().map(estimate_tokens).sum();
        ContextUsageEstimate {
            tokens: usage_tokens + trailing_tokens,
            usage_tokens,
            trailing_tokens,
            last_usage_index: Some(index),
        }
    } else {
        let estimated: u64 = messages.iter().map(estimate_tokens).sum();
        ContextUsageEstimate {
            tokens: estimated,
            usage_tokens: 0,
            trailing_tokens: estimated,
            last_usage_index: None,
        }
    }
}

/// Return whether context usage exceeds the configured compaction threshold.
pub fn should_compact(context_tokens: u64, context_window: u64, settings: CompactionSettings) -> bool {
    if !settings.enabled {
        return false;
    }
    context_tokens > context_window.saturating_sub(settings.reserve_tokens)
}

fn estimate_text_and_image_content_chars(content: &str) -> usize {
    content.chars().count()
}

fn estimate_blocks_chars(blocks: &[elph_ai::ContentBlock]) -> usize {
    let mut chars = 0usize;
    for block in blocks {
        match block {
            elph_ai::ContentBlock::Text { text } => chars += text.chars().count(),
            elph_ai::ContentBlock::Image { .. } => chars += ESTIMATED_IMAGE_CHARS,
        }
    }
    chars
}

/// Estimate token count for one message using a conservative character heuristic.
pub fn estimate_tokens(message: &AgentMessage) -> u64 {
    let chars = match message {
        AgentMessage::Llm(llm) => match llm.as_ref() {
            Message::User { content, .. } => match content {
                elph_ai::UserContent::Text(text) => estimate_text_and_image_content_chars(text),
                elph_ai::UserContent::Blocks(blocks) => estimate_blocks_chars(blocks),
            },
            Message::Assistant(assistant) => {
                let mut chars = 0usize;
                for block in &assistant.content {
                    match block {
                        AssistantContentBlock::Text(text) => chars += text.text.chars().count(),
                        AssistantContentBlock::Thinking(thinking) => {
                            chars += thinking.thinking.chars().count();
                        }
                        AssistantContentBlock::ToolCall(tool_call) => {
                            chars += tool_call.name.chars().count()
                                + safe_json_stringify(&tool_call.arguments).chars().count();
                        }
                    }
                }
                chars
            }
            Message::ToolResult { content, .. } => estimate_blocks_chars(content),
        },
        AgentMessage::Custom(CustomAgentMessage::BashExecution { command, output, .. }) => {
            command.chars().count() + output.as_ref().map(|s| s.chars().count()).unwrap_or(0)
        }
        AgentMessage::Custom(CustomAgentMessage::BranchSummary { summary, .. })
        | AgentMessage::Custom(CustomAgentMessage::CompactionSummary { summary, .. }) => summary.chars().count(),
        AgentMessage::Custom(CustomAgentMessage::Custom { content, .. }) => content
            .as_str()
            .map(estimate_text_and_image_content_chars)
            .unwrap_or_else(|| content.to_string().chars().count()),
    };
    chars.div_ceil(4) as u64
}

fn find_valid_cut_points(entries: &[SessionTreeEntry], start_index: usize, end_index: usize) -> Vec<usize> {
    let mut cut_points = Vec::new();
    for (i, entry) in entries.iter().enumerate().take(end_index).skip(start_index) {
        match entry {
            SessionTreeEntry::Message { message, .. } => match message.role() {
                "bashExecution" | "custom" | "branchSummary" | "compactionSummary" | "user" | "assistant" => {
                    cut_points.push(i);
                }
                _ => {}
            },
            SessionTreeEntry::BranchSummary { .. } | SessionTreeEntry::CustomMessage { .. } => {
                cut_points.push(i);
            }
            _ => {}
        }
    }
    cut_points
}

/// Find the user-visible message that starts the turn containing an entry.
pub fn find_turn_start_index(entries: &[SessionTreeEntry], entry_index: usize, start_index: usize) -> Option<usize> {
    for i in (start_index..=entry_index).rev() {
        match &entries[i] {
            SessionTreeEntry::BranchSummary { .. } | SessionTreeEntry::CustomMessage { .. } => return Some(i),
            SessionTreeEntry::Message { message, .. } => match message.role() {
                "user" | "bashExecution" => return Some(i),
                _ => {}
            },
            _ => {}
        }
    }
    None
}

/// Cut point selected for compaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CutPointResult {
    pub first_kept_entry_index: usize,
    pub turn_start_index: Option<usize>,
    pub is_split_turn: bool,
}

/// Find the compaction cut point that keeps approximately the requested recent-token budget.
pub fn find_cut_point(
    entries: &[SessionTreeEntry],
    start_index: usize,
    end_index: usize,
    keep_recent_tokens: u64,
) -> CutPointResult {
    let cut_points = find_valid_cut_points(entries, start_index, end_index);
    if cut_points.is_empty() {
        return CutPointResult {
            first_kept_entry_index: start_index,
            turn_start_index: None,
            is_split_turn: false,
        };
    }

    let mut accumulated_tokens = 0u64;
    let mut cut_index = cut_points[0];

    for i in (start_index..end_index).rev() {
        let SessionTreeEntry::Message { message, .. } = &entries[i] else {
            continue;
        };
        accumulated_tokens += estimate_tokens(message);
        if accumulated_tokens >= keep_recent_tokens {
            for &candidate in &cut_points {
                if candidate >= i {
                    cut_index = candidate;
                    break;
                }
            }
            break;
        }
    }

    while cut_index > start_index {
        match &entries[cut_index - 1] {
            SessionTreeEntry::Compaction { .. } | SessionTreeEntry::Message { .. } => break,
            _ => cut_index -= 1,
        }
    }

    let is_user_message = matches!(
        &entries[cut_index],
        SessionTreeEntry::Message { message, .. } if message.role() == "user"
    );
    let turn_start_index = if is_user_message {
        None
    } else {
        find_turn_start_index(entries, cut_index, start_index)
    };

    CutPointResult {
        first_kept_entry_index: cut_index,
        turn_start_index,
        is_split_turn: !is_user_message && turn_start_index.is_some(),
    }
}

fn get_message_from_entry(entry: &SessionTreeEntry) -> Option<AgentMessage> {
    match entry {
        SessionTreeEntry::Message { message, .. } => Some(message.clone()),
        SessionTreeEntry::CustomMessage {
            custom_type,
            content,
            display,
            details,
            timestamp,
            ..
        } => {
            let content = match content {
                CustomMessageEntryContent::Text(text) => CustomMessageContent::Text(text.clone()),
                CustomMessageEntryContent::Blocks(blocks) => CustomMessageContent::Blocks(
                    blocks
                        .iter()
                        .map(|block| match block {
                            CustomMessageEntryBlock::Text(text) => CustomMessageBlock::Text(text.clone()),
                            CustomMessageEntryBlock::Image(image) => CustomMessageBlock::Image(image.clone()),
                        })
                        .collect(),
                ),
            };
            Some(create_custom_message(
                custom_type,
                content,
                *display,
                details.clone(),
                timestamp,
            ))
        }
        SessionTreeEntry::BranchSummary {
            summary,
            from_id,
            timestamp,
            ..
        } => Some(create_branch_summary_message(summary, from_id, timestamp)),
        SessionTreeEntry::Compaction {
            summary,
            tokens_before,
            timestamp,
            ..
        } => Some(create_compaction_summary_message(summary, *tokens_before, timestamp)),
        _ => None,
    }
}

fn get_message_from_entry_for_compaction(entry: &SessionTreeEntry) -> Option<AgentMessage> {
    if matches!(entry, SessionTreeEntry::Compaction { .. }) {
        return None;
    }
    get_message_from_entry(entry)
}

fn extract_file_operations(
    messages: &[AgentMessage],
    entries: &[SessionTreeEntry],
    prev_compaction_index: isize,
) -> FileOperations {
    let mut file_ops = create_file_ops();
    if prev_compaction_index >= 0
        && let SessionTreeEntry::Compaction { details, from_hook, .. } = &entries[prev_compaction_index as usize]
        && !from_hook.unwrap_or(false)
        && let Some(Value::Object(obj)) = details
    {
        if let Some(Value::Array(read_files)) = obj.get("readFiles") {
            for file in read_files {
                if let Some(path) = file.as_str() {
                    file_ops.read.insert(path.to_string());
                }
            }
        }
        if let Some(Value::Array(modified_files)) = obj.get("modifiedFiles") {
            for file in modified_files {
                if let Some(path) = file.as_str() {
                    file_ops.edited.insert(path.to_string());
                }
            }
        }
    }
    for msg in messages {
        extract_file_ops_from_message(msg, &mut file_ops);
    }
    file_ops
}

/// Prepare session entries for compaction, or return `None` when compaction is not applicable.
pub fn prepare_compaction(
    path_entries: &[SessionTreeEntry],
    settings: CompactionSettings,
) -> std::result::Result<Option<CompactionPreparation>, CompactionError> {
    if path_entries.is_empty() || matches!(path_entries.last(), Some(SessionTreeEntry::Compaction { .. })) {
        return Ok(None);
    }

    let mut prev_compaction_index = -1isize;
    for (i, entry) in path_entries.iter().enumerate().rev() {
        if matches!(entry, SessionTreeEntry::Compaction { .. }) {
            prev_compaction_index = i as isize;
            break;
        }
    }

    let mut previous_summary = None;
    let mut boundary_start = 0usize;
    if prev_compaction_index >= 0
        && let SessionTreeEntry::Compaction {
            summary,
            first_kept_entry_id,
            ..
        } = &path_entries[prev_compaction_index as usize]
    {
        previous_summary = Some(summary.clone());
        boundary_start = path_entries
            .iter()
            .position(|entry| entry.id() == first_kept_entry_id)
            .unwrap_or((prev_compaction_index as usize) + 1);
    }
    let boundary_end = path_entries.len();
    let tokens_before = estimate_context_tokens(&build_session_context(path_entries).messages).tokens;

    let cut_point = find_cut_point(path_entries, boundary_start, boundary_end, settings.keep_recent_tokens);
    let first_kept_entry = &path_entries[cut_point.first_kept_entry_index];
    let first_kept_entry_id = first_kept_entry.id().to_string();
    if first_kept_entry_id.is_empty() {
        return Err(CompactionError::new(
            CompactionErrorCode::InvalidSession,
            "First kept entry has no UUID - session may need migration",
        ));
    }

    let history_end = if cut_point.is_split_turn {
        cut_point.turn_start_index.unwrap_or(cut_point.first_kept_entry_index)
    } else {
        cut_point.first_kept_entry_index
    };

    let mut messages_to_summarize = Vec::new();
    for entry in &path_entries[boundary_start..history_end] {
        if let Some(msg) = get_message_from_entry_for_compaction(entry) {
            messages_to_summarize.push(msg);
        }
    }

    let mut turn_prefix_messages = Vec::new();
    if cut_point.is_split_turn {
        let turn_start = cut_point.turn_start_index.unwrap_or(cut_point.first_kept_entry_index);
        for entry in &path_entries[turn_start..cut_point.first_kept_entry_index] {
            if let Some(msg) = get_message_from_entry_for_compaction(entry) {
                turn_prefix_messages.push(msg);
            }
        }
    }

    let mut file_ops = extract_file_operations(&messages_to_summarize, path_entries, prev_compaction_index);
    for msg in &turn_prefix_messages {
        extract_file_ops_from_message(msg, &mut file_ops);
    }

    Ok(Some(CompactionPreparation {
        first_kept_entry_id,
        messages_to_summarize,
        turn_prefix_messages,
        is_split_turn: cut_point.is_split_turn,
        tokens_before,
        previous_summary,
        file_ops,
        settings,
    }))
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

async fn generate_turn_prefix_summary(
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

/// Generate compaction summary data from prepared session history.
pub async fn compact(
    preparation: CompactionPreparation,
    models: &Models,
    model: &Model,
    custom_instructions: Option<&str>,
    signal: Option<CancellationToken>,
    thinking_level: Option<ThinkingLevel>,
) -> std::result::Result<CompactionResult, CompactionError> {
    let CompactionPreparation {
        first_kept_entry_id,
        messages_to_summarize,
        turn_prefix_messages,
        is_split_turn,
        tokens_before,
        previous_summary,
        file_ops,
        settings,
    } = preparation;

    if first_kept_entry_id.is_empty() {
        return Err(CompactionError::new(
            CompactionErrorCode::InvalidSession,
            "First kept entry has no UUID - session may need migration",
        ));
    }

    let summary = if is_split_turn && !turn_prefix_messages.is_empty() {
        let history_result = if messages_to_summarize.is_empty() {
            Ok("No prior history.".to_string())
        } else {
            generate_summary(
                &messages_to_summarize,
                models,
                model,
                settings.reserve_tokens,
                signal.clone(),
                custom_instructions,
                previous_summary.as_deref(),
                thinking_level,
            )
            .await
        };
        let history = match history_result {
            Ok(value) => value,
            Err(error) => return Err(error),
        };
        let turn_prefix = generate_turn_prefix_summary(
            &turn_prefix_messages,
            models,
            model,
            settings.reserve_tokens,
            signal,
            thinking_level,
        )
        .await;
        let turn_prefix = match turn_prefix {
            Ok(value) => value,
            Err(error) => return Err(error),
        };
        format!("{history}\n\n---\n\n**Turn Context (split turn):**\n\n{turn_prefix}")
    } else {
        match generate_summary(
            &messages_to_summarize,
            models,
            model,
            settings.reserve_tokens,
            signal,
            custom_instructions,
            previous_summary.as_deref(),
            thinking_level,
        )
        .await
        {
            Ok(value) => value,
            Err(error) => return Err(error),
        }
    };

    let (read_files, modified_files) = compute_file_lists(&file_ops);
    let mut summary_with_files = summary;
    summary_with_files.push_str(&format_file_operations(&read_files, &modified_files));

    Ok(CompactionResult {
        summary: summary_with_files,
        first_kept_entry_id,
        tokens_before,
        details: Some(CompactionDetails {
            read_files,
            modified_files,
        }),
    })
}

fn now_millis() -> i64 {
    (time::OffsetDateTime::now_utc().unix_timestamp_nanos() / 1_000_000) as i64
}
