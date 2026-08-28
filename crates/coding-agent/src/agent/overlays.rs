//! Select-list data for TUI slash-command overlays.

use crate::types::{SelectItem, SelectItemKind};
use anyhow::{Context, Result};
use elph_agent::session::{CustomMessageEntryBlock, CustomMessageEntryContent, SessionTreeEntry};
use elph_ai::{AssistantContentBlock, Message, UserContent};
use elph_ai::{get_builtin_model, get_builtin_providers};
use std::collections::HashSet;

use super::session_info_slash::format_session_timestamp;
use super::session_manager::SessionManager;

pub fn list_model_select_items() -> Vec<SelectItem> {
    let mut items = Vec::new();
    for provider in get_builtin_providers() {
        for model in elph_ai::get_builtin_models(&provider) {
            let value = format!("{provider}/{}", model.id);
            let description = if model.reasoning {
                format!("{provider} · reasoning")
            } else {
                provider.clone()
            };
            items.push(SelectItem::new(value, model.name).with_description(description));
        }
    }
    items.sort_by(|a, b| a.label.cmp(&b.label));
    items
}

pub async fn list_session_select_items(session_manager: &SessionManager) -> Result<Vec<SelectItem>> {
    let sessions = session_manager.list().await?;
    // The `sessions.name` column only carries the create-time name; display
    // titles live in `session_info` tree entries (auto-title / `/rename`).
    let ids: Vec<String> = sessions.iter().map(|m| m.id.clone()).collect();
    let titles = session_manager.session_titles(&ids).await?;
    // Repo already sorts by last activity (`updated_at`); keep that order.
    let items: Vec<SelectItem> = sessions
        .into_iter()
        .map(|meta| {
            let title = titles
                .get(&meta.id)
                .map(String::as_str)
                .or_else(|| meta.name.as_deref().map(str::trim).filter(|s| !s.is_empty()));
            let timestamp = format_session_timestamp(&meta.updated_at);
            match title {
                Some(title) => {
                    SelectItem::new(meta.id.clone(), title).with_description(format!("{} · {timestamp}", meta.id))
                }
                None => SelectItem::new(meta.id.clone(), meta.id.clone()).with_description(timestamp),
            }
        })
        .collect();
    Ok(items)
}

pub fn list_tree_select_items(entries: &[SessionTreeEntry]) -> Vec<SelectItem> {
    list_tree_select_items_with_leaf(entries, None)
}

/// Selectable tree points for `/tree` interactive picker.
///
/// Builds a **full** candidate list (including tools/settings). UI filter modes
/// (Pi TreeSelector) decide what is visible: default / no-tools / user-only /
/// labeled-only / all.
pub fn list_tree_select_items_with_leaf(entries: &[SessionTreeEntry], leaf_id: Option<&str>) -> Vec<SelectItem> {
    let labeled_targets: HashSet<&str> = entries
        .iter()
        .filter_map(|e| match e {
            SessionTreeEntry::Label {
                target_id,
                label: Some(l),
                ..
            } if !l.is_empty() => Some(target_id.as_str()),
            _ => None,
        })
        .collect();

    entries
        .iter()
        .filter_map(|e| tree_entry_to_select_item(e, leaf_id, &labeled_targets))
        .collect()
}

fn tree_entry_to_select_item(
    entry: &SessionTreeEntry,
    leaf_id: Option<&str>,
    labeled_targets: &HashSet<&str>,
) -> Option<SelectItem> {
    let id = entry.id().to_string();
    let leaf_mark = if leaf_id == Some(entry.id()) { "● " } else { "" };
    let is_labeled = labeled_targets.contains(entry.id());
    let label_badge = if is_labeled { " ★" } else { "" };
    let short: String = id.chars().take(8).collect();

    match entry {
        SessionTreeEntry::Message { message, timestamp, .. } => {
            let role = message.role();
            let (kind, role_label) = match role {
                "user" => (SelectItemKind::UserMessage, "user"),
                "assistant" => (SelectItemKind::AssistantMessage, "assistant"),
                "toolResult" | "tool_result" | "tool" => (SelectItemKind::ToolResult, "tool"),
                _ => {
                    // Other message roles treated as tools/settings bookkeeping.
                    (SelectItemKind::ToolResult, role)
                }
            };
            let preview = message_preview(message);
            if preview.is_empty() && kind != SelectItemKind::ToolResult {
                return None;
            }
            let preview = if preview.is_empty() {
                "(empty)".to_string()
            } else {
                preview
            };
            Some(
                SelectItem::new(id, format!("{leaf_mark}{role_label}: {preview}{label_badge}"))
                    .with_description(format!("{short} · {timestamp}"))
                    .with_kind(kind)
                    .with_labeled(is_labeled),
            )
        }
        SessionTreeEntry::CustomMessage {
            content,
            display,
            timestamp,
            custom_type,
            ..
        } if *display => {
            let preview = custom_message_preview(content);
            if preview.is_empty() {
                return None;
            }
            Some(
                SelectItem::new(id, format!("{leaf_mark}{custom_type}: {preview}{label_badge}"))
                    .with_description(format!("{short} · {timestamp}"))
                    .with_kind(SelectItemKind::Generic)
                    .with_labeled(is_labeled),
            )
        }
        SessionTreeEntry::BranchSummary { summary, timestamp, .. } => {
            let preview: String = summary.chars().take(60).collect();
            Some(
                SelectItem::new(id, format!("{leaf_mark}branch: {preview}{label_badge}"))
                    .with_description(format!("{short} · {timestamp}"))
                    .with_kind(SelectItemKind::BranchSummary)
                    .with_labeled(is_labeled),
            )
        }
        SessionTreeEntry::Compaction { summary, timestamp, .. } => {
            let preview: String = summary.chars().take(60).collect();
            Some(
                SelectItem::new(id, format!("{leaf_mark}compaction: {preview}{label_badge}"))
                    .with_description(format!("{short} · {timestamp}"))
                    .with_kind(SelectItemKind::Compaction)
                    .with_labeled(is_labeled),
            )
        }
        SessionTreeEntry::Label {
            target_id,
            label,
            timestamp,
            ..
        } => {
            let name = label.as_deref().filter(|s| !s.is_empty()).unwrap_or("(cleared)");
            let tshort: String = target_id.chars().take(8).collect();
            Some(
                SelectItem::new(id, format!("{leaf_mark}label: {name} → {tshort}"))
                    .with_description(format!("{short} · {timestamp}"))
                    .with_kind(SelectItemKind::Label)
                    .with_labeled(true),
            )
        }
        SessionTreeEntry::ModelChange {
            provider,
            model_id,
            timestamp,
            ..
        } => Some(
            SelectItem::new(id, format!("{leaf_mark}model: {provider}/{model_id}"))
                .with_description(format!("{short} · {timestamp}"))
                .with_kind(SelectItemKind::Settings)
                .with_labeled(is_labeled),
        ),
        SessionTreeEntry::ThinkingLevelChange {
            thinking_level,
            timestamp,
            ..
        } => Some(
            SelectItem::new(id, format!("{leaf_mark}thinking: {thinking_level}"))
                .with_description(format!("{short} · {timestamp}"))
                .with_kind(SelectItemKind::Settings)
                .with_labeled(is_labeled),
        ),
        SessionTreeEntry::SessionInfo { name, timestamp, .. } => {
            let n = name.as_deref().unwrap_or("(unnamed)");
            Some(
                SelectItem::new(id, format!("{leaf_mark}session: {n}"))
                    .with_description(format!("{short} · {timestamp}"))
                    .with_kind(SelectItemKind::Settings)
                    .with_labeled(is_labeled),
            )
        }
        SessionTreeEntry::ActiveToolsChange { timestamp, .. } => Some(
            SelectItem::new(id, format!("{leaf_mark}tools change"))
                .with_description(format!("{short} · {timestamp}"))
                .with_kind(SelectItemKind::Settings)
                .with_labeled(is_labeled),
        ),
        SessionTreeEntry::CollaborationModeChange { mode, timestamp, .. } => Some(
            SelectItem::new(id, format!("{leaf_mark}mode: {mode:?}"))
                .with_description(format!("{short} · {timestamp}"))
                .with_kind(SelectItemKind::Settings)
                .with_labeled(is_labeled),
        ),
        SessionTreeEntry::Custom {
            custom_type, timestamp, ..
        } => Some(
            SelectItem::new(id, format!("{leaf_mark}custom: {custom_type}"))
                .with_description(format!("{short} · {timestamp}"))
                .with_kind(SelectItemKind::Settings)
                .with_labeled(is_labeled),
        ),
        SessionTreeEntry::Leaf { .. } => None,
        SessionTreeEntry::CustomMessage { display: false, .. } => None,
        // Exhaustive: non-display custom messages already covered; any future variant stays hidden.
        #[allow(unreachable_patterns)]
        _ => None,
    }
}

fn message_preview(message: &elph_agent::AgentMessage) -> String {
    match message {
        elph_agent::AgentMessage::Llm(msg) => match msg.as_ref() {
            Message::User { content, .. } => user_content_text(content),
            Message::Assistant(assistant) => assistant_text(assistant),
            Message::ToolResult { tool_name, .. } => tool_name.clone(),
        },
        elph_agent::AgentMessage::Custom(custom) => match custom {
            elph_agent::CustomAgentMessage::BranchSummary { summary, .. } => summary.clone(),
            elph_agent::CustomAgentMessage::CompactionSummary { summary, .. } => summary.clone(),
            elph_agent::CustomAgentMessage::ShellExecExecution { command, .. } => command.clone(),
            elph_agent::CustomAgentMessage::Custom { kind, .. } => kind.clone(),
        },
    }
}

fn user_content_text(content: &UserContent) -> String {
    match content {
        UserContent::Text(text) => truncate_preview(text),
        UserContent::Blocks(blocks) => blocks
            .iter()
            .filter_map(|block| match block {
                elph_ai::ContentBlock::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(" "),
    }
}

fn assistant_text(assistant: &elph_ai::AssistantMessage) -> String {
    assistant
        .content
        .iter()
        .filter_map(|block| match block {
            AssistantContentBlock::Text(text) => Some(text.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(80)
        .collect()
}

fn custom_message_preview(content: &CustomMessageEntryContent) -> String {
    match content {
        CustomMessageEntryContent::Text(text) => truncate_preview(text),
        CustomMessageEntryContent::Blocks(blocks) => blocks
            .iter()
            .filter_map(|block| match block {
                CustomMessageEntryBlock::Text(text) => Some(text.text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(" "),
    }
}

fn truncate_preview(text: &str) -> String {
    let collapsed: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() <= 80 {
        collapsed
    } else {
        let truncated: String = collapsed.chars().take(77).collect();
        format!("{truncated}…")
    }
}

/// Parse `provider/model` selection values from the model selector.
pub fn parse_model_value(value: &str) -> Result<(String, String)> {
    value
        .split_once('/')
        .map(|(provider, model_id)| (provider.to_string(), model_id.to_string()))
        .with_context(|| format!("Invalid model value: {value}"))
}

pub fn resolve_model_from_value(value: &str) -> Result<elph_ai::Model> {
    let (provider, model_id) = parse_model_value(value)?;
    get_builtin_model(&provider, &model_id).with_context(|| format!("Model not found: {value}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use elph_agent::llm_message_to_agent;

    #[test]
    fn tree_items_mark_labeled_targets() {
        let entries = vec![
            SessionTreeEntry::Message {
                id: "m1".into(),
                parent_id: None,
                timestamp: "t".into(),
                message: llm_message_to_agent(Message::User {
                    content: UserContent::Text("hello".into()),
                    timestamp: 0,
                }),
                prompt_title: String::new(),
                prompt_kind: String::new(),
            },
            SessionTreeEntry::Label {
                id: "l1".into(),
                parent_id: Some("m1".into()),
                timestamp: "t".into(),
                target_id: "m1".into(),
                label: Some("bookmark".into()),
            },
        ];
        let items = list_tree_select_items_with_leaf(&entries, Some("m1"));
        let msg = items.iter().find(|i| i.value == "m1").expect("message item");
        assert!(msg.labeled);
        assert_eq!(msg.kind, SelectItemKind::UserMessage);
        assert!(items.iter().any(|i| i.kind == SelectItemKind::Label));
    }
}
