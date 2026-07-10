//! Select-list data for TUI slash-command overlays.

use anyhow::{Context, Result};
use elph_agent::{CustomMessageEntryBlock, CustomMessageEntryContent, SessionTreeEntry};
use elph_ai::{AssistantContentBlock, Message, UserContent, get_builtin_model, get_builtin_providers};
use elph_tui::SelectItem;

use super::session_manager::SessionManager;

pub fn list_model_select_items() -> Vec<SelectItem> {
    let mut items = Vec::new();
    for provider in get_builtin_providers() {
        for model in elph_ai::get_builtin_models(provider) {
            let value = format!("{provider}/{}", model.id);
            let description = if model.reasoning {
                format!("{provider} · reasoning")
            } else {
                provider.to_string()
            };
            items.push(SelectItem::new(value, model.name).with_description(description));
        }
    }
    items.sort_by(|a, b| a.label.cmp(&b.label));
    items
}

pub async fn list_session_select_items(session_manager: &SessionManager) -> Result<Vec<SelectItem>> {
    let sessions = session_manager.list().await?;
    let mut items: Vec<SelectItem> = sessions
        .into_iter()
        .map(|meta| {
            let short_id = meta.id.chars().take(8).collect::<String>();
            SelectItem::new(meta.id, short_id).with_description(meta.created_at)
        })
        .collect();
    items.sort_by(|a, b| b.description.cmp(&a.description));
    Ok(items)
}

pub fn list_tree_select_items(entries: &[SessionTreeEntry]) -> Vec<SelectItem> {
    entries.iter().filter_map(tree_entry_to_select_item).collect()
}

fn tree_entry_to_select_item(entry: &SessionTreeEntry) -> Option<SelectItem> {
    let id = entry.id().to_string();
    match entry {
        SessionTreeEntry::Message { message, timestamp, .. } => {
            let role = message.role();
            if role != "user" && role != "assistant" {
                return None;
            }
            let preview = message_preview(message);
            if preview.is_empty() {
                return None;
            }
            let label = format!("{role}: {preview}");
            Some(SelectItem::new(id, label).with_description(timestamp.clone()))
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
            let label = format!("{custom_type}: {preview}");
            Some(SelectItem::new(id, label).with_description(timestamp.clone()))
        }
        SessionTreeEntry::BranchSummary { summary, timestamp, .. } => {
            Some(SelectItem::new(id, format!("branch: {summary}")).with_description(timestamp.clone()))
        }
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
            elph_agent::CustomAgentMessage::BashExecution { command, .. } => command.clone(),
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
        collapsed.chars().take(77).collect::<String>() + "..."
    }
}

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
