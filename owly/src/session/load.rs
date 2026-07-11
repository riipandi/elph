use anyhow::Result;
use elph_agent::AgentMessage;
use serde_json::Value;

use crate::checkpoint::{
    ASSISTANT_DRAFT, CheckpointPendingWrite, INTERRUPT, RESUME, RunnableConfig, TursoCheckpointSaver,
};

use super::persist::messages_from_checkpoint;
use super::thread::interactive_config;
use super::types::SessionRecovery;

pub(super) async fn load_session_state(
    saver: &TursoCheckpointSaver,
    thread_id: &str,
) -> Result<(RunnableConfig, Vec<AgentMessage>, i64)> {
    let (config, messages, _) = load_messages_with_recovery(saver, thread_id).await?;
    let step = saver
        .get_tuple(&config)
        .await?
        .and_then(|t| t.metadata.map(|m| m.step))
        .unwrap_or(0);
    Ok((config, messages, step))
}

/// Load the latest conversation messages for a thread, if any.
pub async fn load_messages(
    saver: &TursoCheckpointSaver,
    thread_id: &str,
) -> Result<(RunnableConfig, Vec<AgentMessage>)> {
    let (config, messages, _) = load_messages_with_recovery(saver, thread_id).await?;
    Ok((config, messages))
}

/// Load messages for a thread and merge recoverable pending writes.
pub async fn load_messages_with_recovery(
    saver: &TursoCheckpointSaver,
    thread_id: &str,
) -> Result<(RunnableConfig, Vec<AgentMessage>, SessionRecovery)> {
    let config = interactive_config(thread_id);
    let Some(tuple) = saver.get_tuple(&config).await? else {
        return Ok((config, Vec::new(), SessionRecovery::default()));
    };

    let base = messages_from_checkpoint(&tuple.checkpoint);
    let (messages, recovery) = merge_recovery_messages(base, &tuple.pending_writes);
    Ok((tuple.config, messages, recovery))
}

/// Apply assistant-draft and pending-interrupt recovery from checkpoint writes.
pub fn merge_recovery_messages(
    mut messages: Vec<AgentMessage>,
    pending_writes: &[CheckpointPendingWrite],
) -> (Vec<AgentMessage>, SessionRecovery) {
    let mut recovery = SessionRecovery::default();

    if let Some(text) = pending_writes
        .iter()
        .find(|(_, channel, _)| channel == ASSISTANT_DRAFT)
        .and_then(|(_, _, value)| value.get("text"))
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|text| !text.is_empty())
    {
        let already_present = messages.iter().any(|message| {
            message
                .as_llm()
                .and_then(|llm| llm.as_assistant())
                .is_some_and(|assistant| {
                    assistant.content.iter().any(|block| {
                        matches!(
                            block,
                            elph_ai::AssistantContentBlock::Text(content) if content.text == text
                        )
                    })
                })
        });
        if !already_present {
            messages.push(recovered_assistant_message(text));
            recovery.draft_restored = true;
        }
    }

    let has_resume = pending_writes.iter().any(|(_, channel, _)| channel == RESUME);
    if !has_resume
        && let Some(interrupt) = pending_writes
            .iter()
            .find(|(_, channel, _)| channel == INTERRUPT)
            .map(|(_, _, value)| value)
    {
        recovery.pending_interrupt = interrupt_summary(interrupt);
    }

    (messages, recovery)
}

fn interrupt_summary(value: &Value) -> Option<String> {
    let tool = value.get("tool").and_then(|v| v.as_str()).unwrap_or("ask");
    let question = value
        .get("args")
        .and_then(|v| v.as_str())
        .filter(|text| !text.is_empty());
    Some(match question {
        Some(args) => format!("{tool} ({args})"),
        None => tool.to_string(),
    })
}

fn recovered_assistant_message(text: &str) -> AgentMessage {
    use elph_ai::{AssistantContentBlock, AssistantMessage, Message, StopReason, TextContent, Usage};
    elph_agent::llm_message_to_agent(Message::Assistant(AssistantMessage {
        role: "assistant".to_string(),
        content: vec![AssistantContentBlock::Text(TextContent::new(text))],
        api: "recovery".to_string(),
        provider: "recovery".to_string(),
        model: "recovered".to_string(),
        response_model: None,
        response_id: None,
        diagnostics: None,
        usage: Usage::default(),
        stop_reason: StopReason::Stop,
        error_message: None,
        timestamp: chrono::Utc::now().timestamp_millis(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checkpoint::{Checkpoint, CheckpointMetadata, TursoCheckpointSaver};
    use crate::session::interactive_config;
    use serde_json::json;
    use std::collections::HashMap;
    use std::sync::Arc;
    use tempfile::TempDir;

    #[test]
    fn merge_recovery_appends_draft_assistant() {
        let (messages, recovery) = merge_recovery_messages(
            Vec::new(),
            &[(
                "assistant_stream".to_string(),
                ASSISTANT_DRAFT.to_string(),
                json!({ "text": "partial answer" }),
            )],
        );
        assert!(recovery.draft_restored);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].role(), "assistant");
    }

    #[test]
    fn merge_recovery_reports_pending_interrupt() {
        let (_, recovery) = merge_recovery_messages(
            Vec::new(),
            &[(
                "ask-1".to_string(),
                INTERRUPT.to_string(),
                json!({ "tool": "ask_text", "args": "question=Continue?" }),
            )],
        );
        assert!(!recovery.draft_restored);
        assert_eq!(
            recovery.pending_interrupt.as_deref(),
            Some("ask_text (question=Continue?)")
        );
    }

    #[tokio::test]
    async fn load_messages_with_recovery_merges_draft() {
        let dir = TempDir::new().expect("tempdir");
        let path = dir.path().join("recovery.sqlite");
        let saver = Arc::new(TursoCheckpointSaver::open(Some(path)).await.expect("open"));
        let thread_id = "thread-recovery";
        let mut config = interactive_config(thread_id);
        config = saver
            .put(
                &config,
                &Checkpoint::default(),
                &CheckpointMetadata {
                    source: "bootstrap".to_string(),
                    step: 0,
                    parents: HashMap::new(),
                },
            )
            .await
            .expect("bootstrap");
        saver
            .put_writes(
                &config,
                &[(ASSISTANT_DRAFT.to_string(), json!({ "text": "draft text" }))],
                "assistant_stream",
            )
            .await
            .expect("draft write");

        let (_, messages, recovery) = load_messages_with_recovery(saver.as_ref(), thread_id)
            .await
            .expect("load");
        assert!(recovery.draft_restored);
        assert_eq!(messages.len(), 1);
    }
}
