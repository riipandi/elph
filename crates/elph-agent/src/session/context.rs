//! Build agent context from a session branch path.

use elph_ai::Message;

use crate::messages::{
    CustomMessageContent, create_branch_summary_message, create_compaction_summary_message, create_custom_message,
};
use crate::mode::CollaborationMode;
use crate::session::types::{SessionContext, SessionModelRef, SessionTreeEntry};
use crate::types::AgentMessage;

/// Transform session tree entries after the default compaction transform.
pub type ContextEntryTransform = Box<dyn Fn(&[SessionTreeEntry]) -> Vec<SessionTreeEntry> + Send + Sync>;

/// Project a custom session entry into zero or more agent messages for model context.
pub type CustomEntryContextMessageProjector =
    Box<dyn Fn(&SessionTreeEntry, usize, &[SessionTreeEntry]) -> Option<Vec<AgentMessage>> + Send + Sync>;

/// Options for building session context from a branch path.
#[derive(Default)]
pub struct SessionContextBuildOptions {
    /// Additional entry transforms applied after the default compaction transform.
    pub entry_transforms: Vec<ContextEntryTransform>,
    /// Optional custom-entry projectors keyed by custom entry type.
    /// Custom entries are omitted from model context by default.
    pub entry_projectors: std::collections::HashMap<String, CustomEntryContextMessageProjector>,
}

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

fn append_message_with_options(
    messages: &mut Vec<AgentMessage>,
    entry: &SessionTreeEntry,
    index: usize,
    entries: &[SessionTreeEntry],
    options: &SessionContextBuildOptions,
) {
    match entry {
        SessionTreeEntry::Custom { custom_type, .. } => {
            if let Some(projector) = options.entry_projectors.get(custom_type)
                && let Some(projected) = projector(entry, index, entries)
            {
                messages.extend(projected);
            }
        }
        other => append_message(messages, other),
    }
}

/// Default compaction transform: keep the latest compaction entry plus entries from
/// `first_kept_entry_id` onward (including post-compaction messages).
pub fn default_context_entry_transform(path_entries: &[SessionTreeEntry]) -> Vec<SessionTreeEntry> {
    let mut compaction: Option<&SessionTreeEntry> = None;
    for entry in path_entries {
        if matches!(entry, SessionTreeEntry::Compaction { .. }) {
            compaction = Some(entry);
        }
    }
    let Some(compaction) = compaction else {
        return path_entries.to_vec();
    };

    let SessionTreeEntry::Compaction {
        id,
        first_kept_entry_id,
        ..
    } = compaction
    else {
        return path_entries.to_vec();
    };

    let mut entries = vec![compaction.clone()];
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
            entries.push(entry.clone());
        }
    }
    for entry in path_entries.iter().skip(compaction_idx + 1) {
        entries.push(entry.clone());
    }
    entries
}

/// Apply default + optional entry transforms to a branch path.
pub fn build_context_entries(
    path_entries: &[SessionTreeEntry],
    options: &SessionContextBuildOptions,
) -> Vec<SessionTreeEntry> {
    let mut entries = default_context_entry_transform(path_entries);
    for transform in &options.entry_transforms {
        entries = transform(&entries);
    }
    entries
}

fn derive_session_context_state(
    path_entries: &[SessionTreeEntry],
) -> (String, Option<SessionModelRef>, Option<Vec<String>>, CollaborationMode) {
    let mut thinking_level = "off".to_string();
    let mut model = None;
    let mut active_tool_names = None;
    let mut collaboration_mode = CollaborationMode::Default;

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
            _ => {}
        }
    }

    (thinking_level, model, active_tool_names, collaboration_mode)
}

/// Build agent context from a session branch path (default options).
pub fn build_session_context(path_entries: &[SessionTreeEntry]) -> SessionContext {
    build_session_context_with_options(path_entries, &SessionContextBuildOptions::default())
}

/// Build agent context with pluggable entry transforms and custom-entry projectors.
pub fn build_session_context_with_options(
    path_entries: &[SessionTreeEntry],
    options: &SessionContextBuildOptions,
) -> SessionContext {
    let (thinking_level, model, active_tool_names, collaboration_mode) = derive_session_context_state(path_entries);

    let entries = build_context_entries(path_entries, options);
    let mut messages = Vec::new();
    for (index, entry) in entries.iter().enumerate() {
        // Compaction summary is projected as a message when present as an entry.
        if let SessionTreeEntry::Compaction {
            summary,
            tokens_before,
            timestamp,
            ..
        } = entry
        {
            messages.push(create_compaction_summary_message(summary, *tokens_before, timestamp));
            continue;
        }
        append_message_with_options(&mut messages, entry, index, &entries, options);
    }

    SessionContext {
        messages,
        thinking_level,
        model,
        active_tool_names,
        collaboration_mode,
    }
}
