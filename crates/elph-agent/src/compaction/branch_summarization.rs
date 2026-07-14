//! Branch summarization helpers — elph-agent module.

use elph_ai::{Context, Message, Model, Models, SimpleStreamOptions, StopReason};
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::agent::harness::types::FileOperations;
use crate::agent::harness::types::{BranchSummaryError, BranchSummaryErrorCode, BranchSummaryResult};
use crate::compaction::utils::{
    compute_file_lists, create_file_ops, extract_file_ops_from_message, format_file_operations, serialize_conversation,
};
use crate::compaction::{SUMMARIZATION_SYSTEM_PROMPT, estimate_tokens};
use crate::messages::{
    CustomMessageBlock, CustomMessageContent, create_branch_summary_message, create_compaction_summary_message,
    create_custom_message, default_convert_to_llm,
};
use crate::session::tree::Session;
use crate::session::types::{
    CustomMessageEntryBlock, CustomMessageEntryContent, SessionError, SessionErrorCode, SessionStorage,
    SessionTreeEntry,
};
use crate::types::AgentMessage;

/// File-operation details stored on generated branch summary entries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchSummaryDetails {
    pub read_files: Vec<String>,
    pub modified_files: Vec<String>,
}

/// Prepared branch content for summarization.
#[derive(Debug, Clone)]
pub struct BranchPreparation {
    pub messages: Vec<AgentMessage>,
    pub file_ops: FileOperations,
    pub total_tokens: u64,
}

/// Entries selected for branch summarization.
#[derive(Debug, Clone)]
pub struct CollectEntriesResult {
    pub entries: Vec<SessionTreeEntry>,
    pub common_ancestor_id: Option<String>,
}

/// Options for generating a branch summary.
#[derive(Debug, Clone)]
pub struct GenerateBranchSummaryOptions {
    pub signal: Option<CancellationToken>,
    pub custom_instructions: Option<String>,
    pub replace_instructions: bool,
    pub reserve_tokens: u64,
}

impl Default for GenerateBranchSummaryOptions {
    fn default() -> Self {
        Self {
            signal: None,
            custom_instructions: None,
            replace_instructions: false,
            reserve_tokens: 16384,
        }
    }
}

const BRANCH_SUMMARY_PREAMBLE: &str = "The user explored a different conversation branch before returning here.
Summary of that exploration:

";

const BRANCH_SUMMARY_PROMPT: &str =
    "Create a structured summary of this conversation branch for context when returning later.

Use this EXACT format:

## Goal
[What was the user trying to accomplish in this branch?]

## Constraints & Preferences
- [Any constraints, preferences, or requirements mentioned]
- [Or \"(none)\" if none were mentioned]

## Progress
### Done
- [x] [Completed tasks/changes]

### In Progress
- [ ] [Work that was started but not finished]

### Blocked
- [Issues preventing progress, if any]

## Key Decisions
- **[Decision]**: [Brief rationale]

## Next Steps
1. [What should happen next to continue this work]

Keep each section concise. Preserve exact file paths, function names, and error messages.";

fn now_millis() -> i64 {
    (time::OffsetDateTime::now_utc().unix_timestamp_nanos() / 1_000_000) as i64
}

/// Collect entries that should be summarized before navigating to a different session tree entry.
pub async fn collect_entries_for_branch_summary<S: SessionStorage>(
    session: &Session<S>,
    old_leaf_id: Option<&str>,
    target_id: &str,
) -> std::result::Result<CollectEntriesResult, SessionError> {
    let Some(old_leaf_id) = old_leaf_id else {
        return Ok(CollectEntriesResult {
            entries: Vec::new(),
            common_ancestor_id: None,
        });
    };

    let old_path: std::collections::HashSet<String> = session
        .branch(Some(old_leaf_id))
        .await?
        .into_iter()
        .map(|entry| entry.id().to_string())
        .collect();
    let target_path = session.branch(Some(target_id)).await?;

    let mut common_ancestor_id = None;
    for entry in target_path.iter().rev() {
        if old_path.contains(entry.id()) {
            common_ancestor_id = Some(entry.id().to_string());
            break;
        }
    }

    let mut entries = Vec::new();
    let mut current = Some(old_leaf_id.to_string());
    while let Some(entry_id) = current {
        if common_ancestor_id.as_deref() == Some(entry_id.as_str()) {
            break;
        }
        let entry = session
            .entry(&entry_id)
            .await
            .ok_or_else(|| SessionError::new(SessionErrorCode::InvalidEntry, format!("Entry {entry_id} not found")))?;
        current = entry.parent_id().map(str::to_string);
        entries.push(entry);
    }
    entries.reverse();

    Ok(CollectEntriesResult {
        entries,
        common_ancestor_id,
    })
}

fn get_message_from_entry(entry: &SessionTreeEntry) -> Option<AgentMessage> {
    match entry {
        SessionTreeEntry::Message { message, .. } if message.role() == "toolResult" => None,
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

/// Prepare branch entries for summarization within an optional token budget.
pub fn prepare_branch_entries(entries: &[SessionTreeEntry], token_budget: u64) -> BranchPreparation {
    let mut messages = Vec::new();
    let mut file_ops = create_file_ops();
    let mut total_tokens = 0u64;

    for entry in entries {
        if let SessionTreeEntry::BranchSummary { from_hook, details, .. } = entry
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
    }

    for entry in entries.iter().rev() {
        let Some(message) = get_message_from_entry(entry) else {
            continue;
        };
        extract_file_ops_from_message(&message, &mut file_ops);
        let tokens = estimate_tokens(&message);

        if token_budget > 0 && total_tokens + tokens > token_budget {
            if matches!(
                entry,
                SessionTreeEntry::Compaction { .. } | SessionTreeEntry::BranchSummary { .. }
            ) && total_tokens < (token_budget as f64 * 0.9) as u64
            {
                messages.insert(0, message);
                total_tokens += tokens;
            }
            break;
        }

        messages.insert(0, message);
        total_tokens += tokens;
    }

    BranchPreparation {
        messages,
        file_ops,
        total_tokens,
    }
}

/// Generate a summary for abandoned branch entries.
pub async fn generate_branch_summary(
    entries: &[SessionTreeEntry],
    models: &Models,
    model: &Model,
    options: GenerateBranchSummaryOptions,
) -> std::result::Result<BranchSummaryResult, BranchSummaryError> {
    let context_window = model.context_window as u64;
    let token_budget = context_window.saturating_sub(options.reserve_tokens);
    let preparation = prepare_branch_entries(entries, token_budget);

    if preparation.messages.is_empty() {
        return Ok(BranchSummaryResult {
            summary: "No content to summarize".to_string(),
            read_files: Vec::new(),
            modified_files: Vec::new(),
        });
    }

    let llm_messages = default_convert_to_llm(preparation.messages.clone());
    let conversation_text = serialize_conversation(&llm_messages);
    let instructions = match (&options.custom_instructions, options.replace_instructions) {
        (Some(custom), true) => custom.clone(),
        (Some(custom), false) => format!("{BRANCH_SUMMARY_PROMPT}\n\nAdditional focus: {custom}"),
        (None, _) => BRANCH_SUMMARY_PROMPT.to_string(),
    };
    let prompt_text = format!("<conversation>\n{conversation_text}\n</conversation>\n\n{instructions}");

    let mut stream_options = SimpleStreamOptions::from_stream(elph_ai::StreamOptions::default());
    stream_options.base.max_tokens = Some(2048);
    stream_options.base.signal = options.signal;

    let response = models
        .complete_simple(
            model,
            &Context {
                system_prompt: Some(SUMMARIZATION_SYSTEM_PROMPT.to_string()),
                messages: vec![Message::User {
                    content: elph_ai::UserContent::Text(prompt_text),
                    timestamp: now_millis(),
                }],
                tools: None,
            },
            Some(stream_options),
        )
        .await;

    match response.stop_reason {
        StopReason::Aborted => Err(BranchSummaryError::new(
            BranchSummaryErrorCode::Aborted,
            response
                .error_message
                .unwrap_or_else(|| "Branch summary aborted".to_string()),
        )),
        StopReason::Error => Err(BranchSummaryError::new(
            BranchSummaryErrorCode::SummarizationFailed,
            format!(
                "Branch summary failed: {}",
                response.error_message.unwrap_or_else(|| "Unknown error".to_string())
            ),
        )),
        _ => {
            let mut summary = response
                .content
                .iter()
                .filter_map(|block| match block {
                    elph_ai::AssistantContentBlock::Text(text) => Some(text.text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n");
            summary = format!("{BRANCH_SUMMARY_PREAMBLE}{summary}");
            let (read_files, modified_files) = compute_file_lists(&preparation.file_ops);
            summary.push_str(&format_file_operations(&read_files, &modified_files));
            Ok(BranchSummaryResult {
                summary: if summary.is_empty() {
                    "No summary generated".to_string()
                } else {
                    summary
                },
                read_files,
                modified_files,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_options_reserve_tokens() {
        let opts = GenerateBranchSummaryOptions::default();
        assert_eq!(opts.reserve_tokens, 16384);
        assert!(!opts.replace_instructions);
    }

    fn make_user_entry(id: &str, text: &str) -> SessionTreeEntry {
        SessionTreeEntry::Message {
            id: id.into(),
            parent_id: None,
            timestamp: "0".into(),
            message: AgentMessage::Llm(Box::new(Message::User {
                content: elph_ai::UserContent::Text(text.into()),
                timestamp: 0,
            })),
        }
    }

    #[test]
    fn prepare_branch_entries_empty() {
        let result = prepare_branch_entries(&[], 10000);
        assert!(result.messages.is_empty());
        assert_eq!(result.total_tokens, 0);
    }

    #[test]
    fn prepare_branch_entries_within_budget() {
        let entry = make_user_entry("1", "hello world");
        let result = prepare_branch_entries(&[entry], 10000);
        assert_eq!(result.messages.len(), 1);
        assert!(result.total_tokens > 0);
    }

    #[test]
    fn prepare_branch_entries_exceeds_budget() {
        let long = "x".repeat(500);
        let entry = make_user_entry("1", &long);
        let result = prepare_branch_entries(&[entry], 5);
        assert!(result.messages.is_empty());
    }

    #[test]
    fn get_message_from_entry_user() {
        let entry = make_user_entry("1", "hello");
        let result = get_message_from_entry(&entry);
        assert!(result.is_some());
        assert_eq!(result.unwrap().role(), "user");
    }

    #[test]
    fn get_message_from_entry_tool_result_filtered() {
        let entry = SessionTreeEntry::Message {
            id: "1".into(),
            parent_id: None,
            timestamp: "0".into(),
            message: AgentMessage::Llm(Box::new(Message::ToolResult {
                tool_call_id: "t1".into(),
                tool_name: "test".into(),
                content: vec![],
                details: None,
                added_tool_names: None,
                is_error: false,
                timestamp: 0,
            })),
        };
        assert!(get_message_from_entry(&entry).is_none());
    }

    #[test]
    fn get_message_from_entry_compaction() {
        let entry = SessionTreeEntry::Compaction {
            id: "1".into(),
            parent_id: None,
            timestamp: "0".into(),
            summary: "compacted".into(),
            first_kept_entry_id: "first".into(),
            tokens_before: 100,
            details: None,
            from_hook: None,
        };
        let result = get_message_from_entry(&entry);
        assert!(result.is_some());
    }

    #[test]
    fn get_message_from_entry_branch_summary() {
        let entry = SessionTreeEntry::BranchSummary {
            id: "1".into(),
            parent_id: None,
            timestamp: "0".into(),
            from_id: "parent".into(),
            summary: "summary".into(),
            details: None,
            from_hook: None,
        };
        let result = get_message_from_entry(&entry);
        assert!(result.is_some());
    }

    #[test]
    fn prepare_extracts_file_ops_from_details() {
        let details = serde_json::json!({"readFiles": ["r.txt"], "modifiedFiles": ["m.txt"]});
        let entry = SessionTreeEntry::BranchSummary {
            id: "1".into(),
            parent_id: None,
            timestamp: "0".into(),
            from_id: "p".into(),
            summary: "s".into(),
            details: Some(details),
            from_hook: Some(false),
        };
        let result = prepare_branch_entries(&[entry], 10000);
        assert_eq!(result.file_ops.read.len(), 1);
        assert_eq!(result.file_ops.edited.len(), 1);
    }

    #[test]
    fn now_millis_is_reasonable() {
        let t = now_millis();
        assert!(t > 1_700_000_000_000);
    }
}
