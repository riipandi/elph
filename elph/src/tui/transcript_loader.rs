//! Rebuild transcript entries from persisted session branch data.

use elph_agent::{CustomAgentMessage, SessionTreeEntry};
use elph_ai::{AssistantContentBlock, Message, UserContent};
use elph_tui::{ToolExecutionState, ToolExecutionStatus, TranscriptEntry};

pub fn transcript_from_branch(entries: &[SessionTreeEntry], show_thinking: bool) -> Vec<TranscriptEntry> {
    let mut out = Vec::new();
    for entry in entries {
        append_entry(&mut out, entry, show_thinking);
    }
    out
}

fn append_entry(out: &mut Vec<TranscriptEntry>, entry: &SessionTreeEntry, show_thinking: bool) {
    match entry {
        SessionTreeEntry::Message { message, timestamp, .. } => {
            append_message(out, message, Some(timestamp.clone()), show_thinking);
        }
        SessionTreeEntry::CustomMessage {
            content,
            display,
            timestamp,
            ..
        } if *display => {
            let text = custom_message_text(content);
            if !text.is_empty() {
                out.push(TranscriptEntry::user_with_timestamp(text, Some(timestamp.clone())));
            }
        }
        SessionTreeEntry::BranchSummary { summary, .. } if !summary.is_empty() => {
            out.push(TranscriptEntry::system(format!("Branch: {summary}")));
        }
        SessionTreeEntry::Compaction { summary, .. } if !summary.is_empty() => {
            out.push(TranscriptEntry::system(format!("Compacted: {summary}")));
        }
        _ => {}
    }
}

fn append_message(
    out: &mut Vec<TranscriptEntry>,
    message: &elph_agent::AgentMessage,
    timestamp: Option<String>,
    show_thinking: bool,
) {
    match message {
        elph_agent::AgentMessage::Llm(msg) => match msg.as_ref() {
            Message::User { content, .. } => {
                let text = user_content_text(content);
                if !text.is_empty() {
                    out.push(TranscriptEntry::user_with_timestamp(text, timestamp));
                }
            }
            Message::Assistant(assistant) => {
                if show_thinking {
                    let thinking: String = assistant
                        .content
                        .iter()
                        .filter_map(|block| match block {
                            AssistantContentBlock::Thinking(t) => Some(t.thinking.as_str()),
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                        .join("");
                    if !thinking.is_empty() {
                        out.push(TranscriptEntry::thinking(thinking, false));
                    }
                }
                for block in &assistant.content {
                    if let AssistantContentBlock::ToolCall(tc) = block {
                        out.push(TranscriptEntry::tool(
                            ToolExecutionState::new(tc.id.clone(), tc.name.clone())
                                .with_args(tc.arguments.to_string())
                                .with_status(ToolExecutionStatus::Running),
                        ));
                    }
                }
                let text: String = assistant
                    .content
                    .iter()
                    .filter_map(|block| match block {
                        AssistantContentBlock::Text(t) => Some(t.text.as_str()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("");
                if !text.is_empty() {
                    out.push(TranscriptEntry::assistant_with_timestamp(text, timestamp));
                }
            }
            Message::ToolResult {
                tool_call_id,
                tool_name,
                content,
                is_error,
                ..
            } => {
                let output: String = content
                    .iter()
                    .filter_map(|block| match block {
                        elph_ai::ContentBlock::Text { text } => Some(text.as_str()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("");
                if let Some(last) = out.iter_mut().rev().find(|e| {
                    e.tool
                        .as_ref()
                        .is_some_and(|t| t.id == *tool_call_id && t.status == ToolExecutionStatus::Running)
                }) && let Some(tool) = last.tool.as_mut()
                {
                    tool.status = if *is_error {
                        ToolExecutionStatus::Error
                    } else {
                        ToolExecutionStatus::Success
                    };
                    tool.output = output;
                } else {
                    out.push(TranscriptEntry::tool(
                        ToolExecutionState::new(tool_call_id.clone(), tool_name.clone())
                            .with_output(output)
                            .with_status(if *is_error {
                                ToolExecutionStatus::Error
                            } else {
                                ToolExecutionStatus::Success
                            }),
                    ));
                }
            }
        },
        elph_agent::AgentMessage::Custom(custom) => append_custom(out, custom),
    }
}

fn append_custom(out: &mut Vec<TranscriptEntry>, custom: &CustomAgentMessage) {
    match custom {
        CustomAgentMessage::BranchSummary { summary, .. } => {
            out.push(TranscriptEntry::system(format!("Branch: {summary}")));
        }
        CustomAgentMessage::CompactionSummary { summary, .. } => {
            out.push(TranscriptEntry::system(format!("Compacted: {summary}")));
        }
        CustomAgentMessage::BashExecution {
            command,
            output,
            exit_code,
            ..
        } => {
            let mut line = format!("$ {command}");
            if let Some(code) = exit_code {
                line.push_str(&format!(" (exit {code})"));
            }
            if let Some(out_text) = output
                && !out_text.is_empty()
            {
                line.push_str(&format!("\n{out_text}"));
            }
            out.push(TranscriptEntry::system(line));
        }
        CustomAgentMessage::Custom { kind, .. } => {
            out.push(TranscriptEntry::system(kind.to_string()));
        }
    }
}

fn user_content_text(content: &UserContent) -> String {
    match content {
        UserContent::Text(text) => text.clone(),
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

fn custom_message_text(content: &elph_agent::CustomMessageEntryContent) -> String {
    match content {
        elph_agent::CustomMessageEntryContent::Text(text) => text.clone(),
        elph_agent::CustomMessageEntryContent::Blocks(blocks) => blocks
            .iter()
            .filter_map(|block| match block {
                elph_agent::CustomMessageEntryBlock::Text(text) => Some(text.text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(" "),
    }
}
