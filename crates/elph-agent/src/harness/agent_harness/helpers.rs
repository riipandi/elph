//! Harness helpers: navigation options, message builders, validation.

use std::collections::HashMap;

use elph_ai::{AssistantMessage, ImageContent, Message, Model, SimpleStreamOptions, StopReason, UserContent};
use serde_json::json;

use crate::compaction::CompactionResult as CompactionModuleResult;
use crate::harness::types::{
    AgentHarnessError, AgentHarnessErrorCode, AgentHarnessStreamOptions, BranchSummaryError, CompactResult,
    CompactionError,
};
use crate::session::types::{CustomMessageEntryContent, SessionError, SessionTreeEntry};
use crate::types::{AgentMessage, AgentThinkingLevel, AgentTool, llm_message_to_agent};

use super::HarnessOpResult;

/// Options for [`super::AgentHarness::navigate_tree`].
#[derive(Debug, Clone, Default)]
pub struct NavigateTreeOptions {
    pub summarize: bool,
    pub custom_instructions: Option<String>,
    pub replace_instructions: bool,
    pub label: Option<String>,
}

pub(super) fn create_failure_message(model: &Model, error: &str, aborted: bool) -> Message {
    Message::Assistant(AssistantMessage {
        role: "assistant".to_string(),
        content: vec![elph_ai::AssistantContentBlock::Text(elph_ai::TextContent::new(""))],
        api: model.api.clone(),
        provider: model.provider.clone(),
        model: model.id.clone(),
        response_model: None,
        response_id: None,
        usage: elph_ai::Usage::default(),
        stop_reason: if aborted {
            StopReason::Aborted
        } else {
            StopReason::Error
        },
        error_message: Some(error.to_string()),
        timestamp: now_ms(),
    })
}

pub(super) fn create_user_message(text: String, images: Option<Vec<ImageContent>>) -> AgentMessage {
    let mut content = vec![elph_ai::ContentBlock::Text { text }];
    if let Some(images) = images {
        for image in images {
            content.push(elph_ai::ContentBlock::Image {
                data: image.data,
                mime_type: image.mime_type,
            });
        }
    }
    llm_message_to_agent(Message::User {
        content: UserContent::Blocks(content),
        timestamp: now_ms(),
    })
}

pub(super) fn merge_harness_into_simple(
    options: Option<SimpleStreamOptions>,
    harness: &AgentHarnessStreamOptions,
    session_id: &str,
) -> SimpleStreamOptions {
    let mut simple = options.unwrap_or(SimpleStreamOptions {
        base: Default::default(),
        reasoning: None,
        thinking_budgets: None,
    });
    if let Some(transport) = harness.transport {
        simple.base.transport = Some(transport);
    }
    if let Some(timeout_ms) = harness.timeout_ms {
        simple.base.timeout_ms = Some(timeout_ms);
    }
    if let Some(max_retries) = harness.max_retries {
        simple.base.max_retries = Some(max_retries);
    }
    if let Some(max_retry_delay_ms) = harness.max_retry_delay_ms {
        simple.base.max_retry_delay_ms = Some(max_retry_delay_ms);
    }
    if let Some(headers) = &harness.headers {
        simple.base.headers = Some(headers.iter().map(|(k, v)| (k.clone(), Some(v.clone()))).collect());
    }
    if let Some(metadata) = &harness.metadata
        && let serde_json::Value::Object(map) = metadata
    {
        simple.base.metadata = Some(map.iter().map(|(k, v)| (k.clone(), v.clone())).collect());
    }
    simple.base.session_id = Some(session_id.to_string());
    simple
}

fn find_duplicate_names(names: &[String]) -> Vec<String> {
    let mut seen = HashMap::new();
    let mut duplicates = Vec::new();
    for name in names {
        if seen.insert(name.clone(), true).is_some() {
            duplicates.push(name.clone());
        }
    }
    duplicates
}

pub(super) fn validate_unique_names(names: Vec<String>, message: &str) -> HarnessOpResult<()> {
    let duplicates = find_duplicate_names(&names);
    if !duplicates.is_empty() {
        return Err(AgentHarnessError::new(
            AgentHarnessErrorCode::InvalidArgument,
            format!("{message}: {}", duplicates.join(", ")),
        ));
    }
    Ok(())
}

pub(super) fn validate_tool_names(tool_names: &[String], tools: &HashMap<String, AgentTool>) -> HarnessOpResult<()> {
    validate_unique_names(tool_names.to_vec(), "Duplicate active tool name(s)")?;
    let missing: Vec<_> = tool_names.iter().filter(|name| !tools.contains_key(*name)).collect();
    if !missing.is_empty() {
        return Err(AgentHarnessError::new(
            AgentHarnessErrorCode::InvalidArgument,
            format!(
                "Unknown tool(s): {}",
                missing.into_iter().cloned().collect::<Vec<_>>().join(", ")
            ),
        ));
    }
    Ok(())
}

pub(super) fn thinking_level_to_session_string(level: AgentThinkingLevel) -> String {
    match level {
        AgentThinkingLevel::Off => "off".to_string(),
        AgentThinkingLevel::Minimal => "minimal".to_string(),
        AgentThinkingLevel::Low => "low".to_string(),
        AgentThinkingLevel::Medium => "medium".to_string(),
        AgentThinkingLevel::High => "high".to_string(),
        AgentThinkingLevel::Xhigh => "xhigh".to_string(),
    }
}

pub(super) fn module_to_compact_result(result: CompactionModuleResult) -> CompactResult {
    CompactResult {
        summary: result.summary,
        first_kept_entry_id: result.first_kept_entry_id,
        tokens_before: result.tokens_before,
        details: result.details.map(|details| {
            json!({
                "readFiles": details.read_files,
                "modifiedFiles": details.modified_files,
            })
        }),
    }
}

pub(super) fn editor_state_for_target(entry: &SessionTreeEntry) -> (Option<String>, Option<String>) {
    match entry {
        SessionTreeEntry::Message { message, parent_id, .. } if message.role() == "user" => {
            let editor_text = user_message_text(message);
            (parent_id.clone(), editor_text)
        }
        SessionTreeEntry::CustomMessage { content, parent_id, .. } => {
            let editor_text = match content {
                CustomMessageEntryContent::Text(text) => Some(text.clone()),
                CustomMessageEntryContent::Blocks(blocks) => {
                    let text = blocks
                        .iter()
                        .filter_map(|block| match block {
                            crate::session::types::CustomMessageEntryBlock::Text(text) => Some(text.text.as_str()),
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                        .join("");
                    if text.is_empty() { None } else { Some(text) }
                }
            };
            (parent_id.clone(), editor_text)
        }
        _ => (Some(entry.id().to_string()), None),
    }
}

fn user_message_text(message: &AgentMessage) -> Option<String> {
    let llm = message.as_llm()?;
    let Message::User { content, .. } = llm else {
        return None;
    };
    match content {
        UserContent::Text(text) => Some(text.clone()),
        UserContent::Blocks(blocks) => {
            let text = blocks
                .iter()
                .filter_map(|block| match block {
                    elph_ai::ContentBlock::Text { text } => Some(text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("");
            if text.is_empty() { None } else { Some(text) }
        }
    }
}

pub(super) fn session_error(error: SessionError) -> AgentHarnessError {
    AgentHarnessError::new(AgentHarnessErrorCode::Session, error.to_string())
}

pub(super) fn compaction_error(error: CompactionError) -> AgentHarnessError {
    AgentHarnessError::new(AgentHarnessErrorCode::Compaction, error.to_string())
}

pub(super) fn branch_summary_error(error: BranchSummaryError) -> AgentHarnessError {
    AgentHarnessError::new(AgentHarnessErrorCode::BranchSummary, error.to_string())
}

pub(super) fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
