//! Compaction preparation from session entries.

use serde_json::Value;

use crate::compaction::utils::{create_file_ops, extract_file_ops_from_message};
use crate::harness::types::{CompactionError, CompactionErrorCode, FileOperations};

pub use crate::harness::types::{CompactionPreparation, CompactionSettings};

use crate::messages::{
    CustomMessageBlock, CustomMessageContent, create_branch_summary_message, create_compaction_summary_message,
    create_custom_message,
};
use crate::session::build_session_context;
use crate::session::types::{CustomMessageEntryBlock, CustomMessageEntryContent, SessionTreeEntry};
use crate::types::AgentMessage;

use super::estimation::{estimate_context_tokens, find_cut_point};

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
            "First kept entry has no TSID - session may need migration",
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
