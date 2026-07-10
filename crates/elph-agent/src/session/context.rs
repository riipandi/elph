//! Build agent context from a session branch path.

use elph_ai::Message;

use crate::messages::{
    CustomMessageContent, create_branch_summary_message, create_compaction_summary_message, create_custom_message,
};
use crate::mode::CollaborationMode;
use crate::session::types::{SessionContext, SessionModelRef, SessionTreeEntry};
use crate::types::AgentMessage;

fn append_message(messages: &mut Vec<AgentMessage>, entry: &SessionTreeEntry) {
    match entry {
        SessionTreeEntry::Message { message, .. } => messages.push(message.clone()),
        SessionTreeEntry::CustomMessage {
            custom_type,
            content,
            display,
            details,
            timestamp,
            ..
        } => {
            let content = match content {
                crate::session::types::CustomMessageEntryContent::Text(text) => {
                    CustomMessageContent::Text(text.clone())
                }
                crate::session::types::CustomMessageEntryContent::Blocks(blocks) => CustomMessageContent::Blocks(
                    blocks
                        .iter()
                        .map(|block| match block {
                            crate::session::types::CustomMessageEntryBlock::Text(text) => {
                                crate::messages::CustomMessageBlock::Text(text.clone())
                            }
                            crate::session::types::CustomMessageEntryBlock::Image(image) => {
                                crate::messages::CustomMessageBlock::Image(image.clone())
                            }
                        })
                        .collect(),
                ),
            };
            messages.push(create_custom_message(
                custom_type,
                content,
                *display,
                details.clone(),
                timestamp,
            ));
        }
        SessionTreeEntry::BranchSummary {
            summary,
            from_id,
            timestamp,
            ..
        } if !summary.is_empty() => {
            messages.push(create_branch_summary_message(summary, from_id, timestamp));
        }
        _ => {}
    }
}

pub fn build_session_context(path_entries: &[SessionTreeEntry]) -> SessionContext {
    let mut thinking_level = "off".to_string();
    let mut model = None;
    let mut active_tool_names = None;
    let mut collaboration_mode = CollaborationMode::Default;
    let mut compaction: Option<&SessionTreeEntry> = None;

    for entry in path_entries {
        match entry {
            SessionTreeEntry::ThinkingLevelChange {
                thinking_level: level, ..
            } => {
                thinking_level = level.clone();
            }
            SessionTreeEntry::ModelChange { provider, model_id, .. } => {
                model = Some(SessionModelRef {
                    provider: provider.clone(),
                    model_id: model_id.clone(),
                });
            }
            SessionTreeEntry::Message {
                message: AgentMessage::Llm(llm),
                ..
            } if matches!(llm.as_ref(), Message::Assistant(_)) => {
                if let Message::Assistant(assistant) = llm.as_ref() {
                    model = Some(SessionModelRef {
                        provider: assistant.provider.to_string(),
                        model_id: assistant.model.clone(),
                    });
                }
            }
            SessionTreeEntry::ActiveToolsChange {
                active_tool_names: names,
                ..
            } => {
                active_tool_names = Some(names.clone());
            }
            SessionTreeEntry::CollaborationModeChange { mode, .. } => {
                collaboration_mode = *mode;
            }
            SessionTreeEntry::Compaction { .. } => compaction = Some(entry),
            _ => {}
        }
    }

    let mut messages = Vec::new();

    if let Some(SessionTreeEntry::Compaction {
        id,
        summary,
        first_kept_entry_id,
        tokens_before,
        timestamp,
        ..
    }) = compaction
    {
        messages.push(create_compaction_summary_message(summary, *tokens_before, timestamp));
        let compaction_idx = path_entries
            .iter()
            .position(|entry| entry.entry_type() == "compaction" && entry.id() == id)
            .unwrap_or(0);
        let mut found_first_kept = false;
        for entry in path_entries.iter().take(compaction_idx) {
            if entry.id() == first_kept_entry_id {
                found_first_kept = true;
            }
            if found_first_kept {
                append_message(&mut messages, entry);
            }
        }
        for entry in path_entries.iter().skip(compaction_idx + 1) {
            append_message(&mut messages, entry);
        }
    } else {
        for entry in path_entries {
            append_message(&mut messages, entry);
        }
    }

    SessionContext {
        messages,
        thinking_level,
        model,
        active_tool_names,
        collaboration_mode,
    }
}
