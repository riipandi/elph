use anyhow::Result;
use serde_json::json;

use crate::checkpoint::{ASSISTANT_DRAFT, INTERRUPT, RESUME, TOOL_PARTIAL};

use super::thread::tool_write_channel;
use super::types::TurnWriteContext;

impl TurnWriteContext {
    pub async fn record_tool_result(
        &self,
        tool_call_id: &str,
        tool_name: &str,
        args_summary: &str,
        is_error: bool,
        output: &str,
    ) -> Result<()> {
        if self.config.configurable.checkpoint_id.is_none() {
            return Ok(());
        }
        let channel = tool_write_channel(tool_name);
        let value = json!({
            "id": tool_call_id,
            "name": tool_name,
            "args": args_summary,
            "is_error": is_error,
            "output": output,
        });
        self.saver
            .put_writes(&self.config, &[(channel, value)], tool_call_id)
            .await?;
        Ok(())
    }

    /// Persist accumulated assistant text for mid-turn crash recovery.
    pub async fn record_assistant_delta(&self, delta: &str) -> Result<()> {
        if delta.is_empty() || self.config.configurable.checkpoint_id.is_none() {
            return Ok(());
        }
        let draft_text = {
            let mut draft = self.assistant_draft.lock().await;
            draft.push_str(delta);
            draft.clone()
        };
        let value = json!({ "text": draft_text });
        self.saver
            .put_writes(
                &self.config,
                &[(ASSISTANT_DRAFT.to_string(), value)],
                "assistant_stream",
            )
            .await?;
        Ok(())
    }

    /// Persist streaming/partial tool output (replaces prior partial for this turn).
    pub async fn record_tool_partial(
        &self,
        tool_call_id: &str,
        tool_name: &str,
        args_summary: &str,
        output: &str,
    ) -> Result<()> {
        if output.is_empty() || self.config.configurable.checkpoint_id.is_none() {
            return Ok(());
        }
        let value = json!({
            "id": tool_call_id,
            "name": tool_name,
            "args": args_summary,
            "output": output,
        });
        self.saver
            .put_writes(&self.config, &[(TOOL_PARTIAL.to_string(), value)], tool_call_id)
            .await?;
        Ok(())
    }

    /// Record a human-input interrupt before an ask_* tool blocks.
    pub async fn record_interrupt(&self, tool_call_id: &str, tool_name: &str, args_summary: &str) -> Result<()> {
        if self.config.configurable.checkpoint_id.is_none() {
            return Ok(());
        }
        let value = json!({
            "id": tool_call_id,
            "tool": tool_name,
            "args": args_summary,
        });
        self.saver
            .put_writes(&self.config, &[(INTERRUPT.to_string(), value)], tool_call_id)
            .await?;
        Ok(())
    }

    /// Record resume after the user answers an ask_* tool.
    pub async fn record_resume(&self, tool_call_id: &str, tool_name: &str, answer: &str, is_error: bool) -> Result<()> {
        if self.config.configurable.checkpoint_id.is_none() {
            return Ok(());
        }
        let value = json!({
            "id": tool_call_id,
            "tool": tool_name,
            "answer": answer,
            "is_error": is_error,
        });
        self.saver
            .put_writes(&self.config, &[(RESUME.to_string(), value)], tool_call_id)
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checkpoint::{Checkpoint, CheckpointMetadata, TursoCheckpointSaver};
    use crate::session::{MESSAGES_CHANNEL, TOOL_CHANNEL_PREFIX, interactive_config};
    use std::collections::HashMap;
    use std::sync::Arc;
    use tempfile::TempDir;
    use tokio::sync::Mutex;

    #[tokio::test]
    async fn turn_write_context_records_tool_pending_write() {
        let dir = TempDir::new().expect("tempdir");
        let path = dir.path().join("test.sqlite");
        let saver = Arc::new(TursoCheckpointSaver::open(Some(path)).await.expect("open"));
        let thread_id = "thread-tool-writes";
        let mut config = interactive_config(thread_id);
        let checkpoint = Checkpoint::default();
        config = saver
            .put(
                &config,
                &checkpoint,
                &CheckpointMetadata {
                    source: "bootstrap".to_string(),
                    step: 0,
                    parents: HashMap::new(),
                },
            )
            .await
            .expect("bootstrap put");

        let ctx = TurnWriteContext {
            saver: Arc::clone(&saver),
            config: config.clone(),
            assistant_draft: Arc::new(Mutex::new(String::new())),
        };
        ctx.record_tool_result("call-1", "write", r#"{"path":"a.md"}"#, false, "Wrote 10 bytes")
            .await
            .expect("tool write");

        let tuple = saver.get_tuple(&config).await.expect("get").expect("tuple");
        assert_eq!(tuple.pending_writes.len(), 1);
        assert_eq!(tuple.pending_writes[0].0, "call-1");
        assert_eq!(tuple.pending_writes[0].1, tool_write_channel("write"));
        assert_eq!(
            tuple.pending_writes[0].2.get("output").and_then(|v| v.as_str()),
            Some("Wrote 10 bytes")
        );
        assert!(tool_write_channel("bash").starts_with(TOOL_CHANNEL_PREFIX));
        let _ = MESSAGES_CHANNEL;
    }

    #[tokio::test]
    async fn turn_write_context_records_assistant_draft() {
        let dir = TempDir::new().expect("tempdir");
        let path = dir.path().join("draft.sqlite");
        let saver = Arc::new(TursoCheckpointSaver::open(Some(path)).await.expect("open"));
        let thread_id = "thread-draft";
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

        let ctx = TurnWriteContext {
            saver: Arc::clone(&saver),
            config: config.clone(),
            assistant_draft: Arc::new(Mutex::new(String::new())),
        };
        ctx.record_assistant_delta("Hello").await.expect("first delta");
        ctx.record_assistant_delta(" world").await.expect("second delta");

        let tuple = saver.get_tuple(&config).await.expect("get").expect("tuple");
        assert_eq!(tuple.pending_writes.len(), 1);
        assert_eq!(tuple.pending_writes[0].1, ASSISTANT_DRAFT);
        assert_eq!(
            tuple.pending_writes[0].2.get("text").and_then(|v| v.as_str()),
            Some("Hello world")
        );
    }

    #[tokio::test]
    async fn turn_write_context_records_interrupt_and_resume() {
        let dir = TempDir::new().expect("tempdir");
        let path = dir.path().join("interrupt.sqlite");
        let saver = Arc::new(TursoCheckpointSaver::open(Some(path)).await.expect("open"));
        let thread_id = "thread-interrupt";
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

        let ctx = TurnWriteContext {
            saver: Arc::clone(&saver),
            config: config.clone(),
            assistant_draft: Arc::new(Mutex::new(String::new())),
        };
        ctx.record_interrupt("ask-1", "ask_text", "question=Name?")
            .await
            .expect("interrupt");
        ctx.record_resume("ask-1", "ask_text", "Alice", false)
            .await
            .expect("resume");

        let tuple = saver.get_tuple(&config).await.expect("get").expect("tuple");
        assert!(tuple.pending_writes.iter().any(|(_, ch, _)| ch == INTERRUPT));
        assert!(tuple.pending_writes.iter().any(|(_, ch, _)| ch == RESUME));
    }

    #[tokio::test]
    async fn turn_write_context_tool_partial_replaces_latest() {
        let dir = TempDir::new().expect("tempdir");
        let path = dir.path().join("partial.sqlite");
        let saver = Arc::new(TursoCheckpointSaver::open(Some(path)).await.expect("open"));
        let thread_id = "thread-partial";
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

        let ctx = TurnWriteContext {
            saver: Arc::clone(&saver),
            config: config.clone(),
            assistant_draft: Arc::new(Mutex::new(String::new())),
        };
        ctx.record_tool_partial("call-1", "bash", "{}", "line 1")
            .await
            .expect("partial1");
        ctx.record_tool_partial("call-1", "bash", "{}", "line 1\nline 2")
            .await
            .expect("partial2");

        let tuple = saver.get_tuple(&config).await.expect("get").expect("tuple");
        assert_eq!(tuple.pending_writes.len(), 1);
        assert_eq!(tuple.pending_writes[0].1, TOOL_PARTIAL);
        assert_eq!(
            tuple.pending_writes[0].2.get("output").and_then(|v| v.as_str()),
            Some("line 1\nline 2")
        );
    }
}
