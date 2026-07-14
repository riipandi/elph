//! Compaction utility helpers — elph-agent module.

use std::collections::HashSet;

use elph_ai::{AssistantContentBlock, ContentBlock, Message, UserContent};
use serde_json::Value;

use crate::agent::harness::types::FileOperations;
use crate::types::AgentMessage;

/// Create an empty file-operation accumulator.
pub fn create_file_ops() -> FileOperations {
    FileOperations::default()
}

/// Add file operations from assistant tool calls to an accumulator.
pub fn extract_file_ops_from_message(message: &AgentMessage, file_ops: &mut FileOperations) {
    let AgentMessage::Llm(llm) = message else {
        return;
    };
    let Message::Assistant(assistant) = llm.as_ref() else {
        return;
    };

    for block in &assistant.content {
        let AssistantContentBlock::ToolCall(tool_call) = block else {
            continue;
        };
        let Some(path) = tool_call.arguments.get("path").and_then(Value::as_str) else {
            continue;
        };
        match tool_call.name.as_str() {
            "read_file" => {
                file_ops.read.insert(path.to_string());
            }
            "write_file" => {
                file_ops.written.insert(path.to_string());
            }
            "edit_file" => {
                file_ops.edited.insert(path.to_string());
            }
            _ => {}
        }
    }
}

/// Compute sorted read-only and modified file lists from accumulated operations.
pub fn compute_file_lists(file_ops: &FileOperations) -> (Vec<String>, Vec<String>) {
    let modified: HashSet<String> = file_ops.edited.iter().chain(file_ops.written.iter()).cloned().collect();
    let mut read_only: Vec<String> = file_ops
        .read
        .iter()
        .filter(|path| !modified.contains(*path))
        .cloned()
        .collect();
    let mut modified_files: Vec<String> = modified.into_iter().collect();
    read_only.sort();
    modified_files.sort();
    (read_only, modified_files)
}

/// Format file lists as summary metadata tags.
pub fn format_file_operations(read_files: &[String], modified_files: &[String]) -> String {
    let mut sections = Vec::new();
    if !read_files.is_empty() {
        sections.push(format!("<read-files>\n{}\n</read-files>", read_files.join("\n")));
    }
    if !modified_files.is_empty() {
        sections.push(format!("<modified-files>\n{}\n</modified-files>", modified_files.join("\n")));
    }
    if sections.is_empty() {
        String::new()
    } else {
        format!("\n\n{}", sections.join("\n\n"))
    }
}

const TOOL_RESULT_MAX_CHARS: usize = 2000;

fn safe_json_stringify(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "[unserializable]".to_string())
}

fn truncate_for_summary(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let truncated_chars = text.chars().count() - max_chars;
    let prefix: String = text.chars().take(max_chars).collect();
    format!("{prefix}\n\n[... {truncated_chars} more characters truncated]")
}

fn extract_text_from_user_content(content: &UserContent) -> String {
    match content {
        UserContent::Text(text) => text.clone(),
        UserContent::Blocks(blocks) => blocks
            .iter()
            .filter_map(|block| match block {
                ContentBlock::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(""),
    }
}

fn extract_text_from_content_blocks(blocks: &[ContentBlock]) -> String {
    blocks
        .iter()
        .filter_map(|block| match block {
            ContentBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("")
}

/// Serialize LLM messages to plain text for summarization prompts.
pub fn serialize_conversation(messages: &[Message]) -> String {
    let mut parts = Vec::new();

    for msg in messages {
        match msg {
            Message::User { content, .. } => {
                let content = extract_text_from_user_content(content);
                if !content.is_empty() {
                    parts.push(format!("[User]: {content}"));
                }
            }
            Message::Assistant(assistant) => {
                let mut text_parts = Vec::new();
                let mut thinking_parts = Vec::new();
                let mut tool_calls = Vec::new();

                for block in &assistant.content {
                    match block {
                        AssistantContentBlock::Text(text) => text_parts.push(text.text.clone()),
                        AssistantContentBlock::Thinking(thinking) => {
                            thinking_parts.push(thinking.thinking.clone());
                        }
                        AssistantContentBlock::ToolCall(tool_call) => {
                            let args_str = tool_call
                                .arguments
                                .as_object()
                                .map(|obj| {
                                    obj.iter()
                                        .map(|(k, v)| format!("{k}={}", safe_json_stringify(v)))
                                        .collect::<Vec<_>>()
                                        .join(", ")
                                })
                                .unwrap_or_else(|| safe_json_stringify(&tool_call.arguments));
                            tool_calls.push(format!("{}({args_str})", tool_call.name));
                        }
                    }
                }

                if !thinking_parts.is_empty() {
                    parts.push(format!("[Assistant thinking]: {}", thinking_parts.join("\n")));
                }
                if !text_parts.is_empty() {
                    parts.push(format!("[Assistant]: {}", text_parts.join("\n")));
                }
                if !tool_calls.is_empty() {
                    parts.push(format!("[Assistant tool calls]: {}", tool_calls.join("; ")));
                }
            }
            Message::ToolResult { content, .. } => {
                let content = extract_text_from_content_blocks(content);
                if !content.is_empty() {
                    parts.push(format!(
                        "[Tool result]: {}",
                        truncate_for_summary(&content, TOOL_RESULT_MAX_CHARS)
                    ));
                }
            }
        }
    }

    parts.join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use elph_ai::{Message, UserContent};

    #[test]
    fn compute_file_lists_separates_read_and_write() {
        let mut ops = create_file_ops();
        ops.read.insert("read.txt".into());
        ops.written.insert("write.txt".into());
        let (reads, writes) = compute_file_lists(&ops);
        assert_eq!(reads, vec!["read.txt"]);
        assert_eq!(writes, vec!["write.txt"]);
    }

    #[test]
    fn compute_file_lists_empty() {
        let ops = create_file_ops();
        let (reads, writes) = compute_file_lists(&ops);
        assert!(reads.is_empty());
        assert!(writes.is_empty());
    }

    #[test]
    fn compute_file_lists_edited_appears_in_modified() {
        let mut ops = create_file_ops();
        ops.read.insert("read.txt".into());
        ops.edited.insert("edit.txt".into());
        let (reads, writes) = compute_file_lists(&ops);
        assert_eq!(reads, vec!["read.txt"]);
        assert_eq!(writes, vec!["edit.txt"]);
    }

    #[test]
    fn compute_file_lists_read_also_written_excluded_from_read_only() {
        let mut ops = create_file_ops();
        ops.read.insert("both.txt".into());
        ops.written.insert("both.txt".into());
        let (reads, writes) = compute_file_lists(&ops);
        assert!(reads.is_empty());
        assert_eq!(writes, vec!["both.txt"]);
    }

    #[test]
    fn format_file_operations_both() {
        let result = format_file_operations(&["r1".into()], &["w1".into()]);
        assert!(result.contains("<read-files>"));
        assert!(result.contains("<modified-files>"));
        assert!(result.contains("r1"));
        assert!(result.contains("w1"));
    }

    #[test]
    fn format_file_operations_empty() {
        let result = format_file_operations(&[], &[]);
        assert_eq!(result, "");
    }

    #[test]
    fn truncate_for_summary_short() {
        assert_eq!(truncate_for_summary("hello", 100), "hello");
    }

    #[test]
    fn truncate_for_summary_long() {
        let long = "x".repeat(200);
        let result = truncate_for_summary(&long, 100);
        // verify result is not the full original
        assert!(result.len() < long.len());
    }

    #[test]
    fn extract_text_from_user_content_text() {
        let content = UserContent::Text("hello world".into());
        assert_eq!(extract_text_from_user_content(&content), "hello world");
    }

    #[test]
    fn serialize_conversation_user_message() {
        let messages = vec![Message::User {
            content: UserContent::Text("hello".into()),
            timestamp: 0,
        }];
        let result = serialize_conversation(&messages);
        assert!(result.contains("[User]"));
        assert!(result.contains("hello"));
    }
}
