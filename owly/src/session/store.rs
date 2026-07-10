use anyhow::{Context, Result};
use elph_agent::AgentMessage;
use elph_agent::create_tsid;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::checkpoint::{
    Checkpoint, CheckpointConfigurable, CheckpointListOptions, CheckpointMetadata, CheckpointTuple, RunnableConfig,
    TursoCheckpointSaver,
};

use super::load::{load_messages_with_recovery, load_session_state};
use super::persist::{messages_from_checkpoint, save_messages};
use super::thread::{create_session_thread_id, interactive_config};
use super::types::{CheckpointSummary, LoadedConversation, TurnWriteContext};

/// Turso-backed session with checkpoint persistence.
pub struct SessionStore {
    saver: Arc<TursoCheckpointSaver>,
    thread_id: String,
    checkpoint_config: RunnableConfig,
    step: i64,
}

impl SessionStore {
    pub async fn open(cwd: &Path) -> Result<Self> {
        Self::open_with_path(crate::checkpoint::default_db_path(), cwd).await
    }

    /// Open a session against an explicit database path.
    pub async fn open_with_path(db_path: impl AsRef<Path>, cwd: &Path) -> Result<Self> {
        let saver = Arc::new(TursoCheckpointSaver::open(Some(db_path.as_ref().to_path_buf())).await?);
        let thread_id = create_session_thread_id(cwd, None);
        let (checkpoint_config, _, step) = load_session_state(saver.as_ref(), &thread_id).await?;
        let mut store = Self {
            saver,
            thread_id,
            checkpoint_config,
            step,
        };
        store.ensure_bootstrap_checkpoint().await?;
        Ok(store)
    }

    pub async fn with_new_thread(cwd: &Path) -> Result<Self> {
        let saver = Arc::new(TursoCheckpointSaver::default().await?);
        let thread_id = create_session_thread_id(cwd, Some(create_tsid().as_str()));
        let checkpoint_config = interactive_config(&thread_id);
        let mut store = Self {
            saver,
            thread_id,
            checkpoint_config,
            step: 0,
        };
        store.ensure_bootstrap_checkpoint().await?;
        Ok(store)
    }

    pub fn thread_id(&self) -> &str {
        &self.thread_id
    }

    pub fn db_path(&self) -> &Path {
        self.saver.db_path()
    }

    pub async fn load_messages(&self) -> Result<Vec<AgentMessage>> {
        Ok(self.load_conversation().await?.messages)
    }

    /// Load messages and apply crash-recovery merges from pending writes.
    pub async fn load_conversation(&self) -> Result<LoadedConversation> {
        if self.checkpoint_config.configurable.checkpoint_id.is_some()
            && let Some(tuple) = self.saver.get_tuple(&self.checkpoint_config).await?
        {
            let base = messages_from_checkpoint(&tuple.checkpoint);
            let (messages, recovery) = super::load::merge_recovery_messages(base, &tuple.pending_writes);
            return Ok(LoadedConversation { messages, recovery });
        }
        let (_, messages, recovery) = load_messages_with_recovery(self.saver.as_ref(), &self.thread_id).await?;
        Ok(LoadedConversation { messages, recovery })
    }

    /// Frozen checkpoint handle for the current turn (before [`Self::save_messages`]).
    pub fn turn_write_context(&self) -> TurnWriteContext {
        TurnWriteContext {
            saver: Arc::clone(&self.saver),
            config: self.checkpoint_config.clone(),
            assistant_draft: Arc::new(Mutex::new(String::new())),
        }
    }

    /// Ensure a root checkpoint exists so turn-1 `put_writes` has a target.
    pub async fn ensure_bootstrap_checkpoint(&mut self) -> Result<()> {
        if self.checkpoint_config.configurable.checkpoint_id.is_some() {
            return Ok(());
        }
        let checkpoint = Checkpoint::default();
        let metadata = CheckpointMetadata {
            source: "bootstrap".to_string(),
            step: 0,
            parents: HashMap::new(),
        };
        self.checkpoint_config = self.saver.put(&self.checkpoint_config, &checkpoint, &metadata).await?;
        Ok(())
    }

    pub async fn save_messages(&mut self, messages: &[AgentMessage], source: &str) -> Result<()> {
        self.step += 1;
        self.checkpoint_config = save_messages(
            self.saver.as_ref(),
            &self.checkpoint_config,
            messages,
            self.step,
            source,
        )
        .await?;
        Ok(())
    }

    /// Start a fresh thread id and bootstrap a root checkpoint.
    pub async fn reset_thread(&mut self, cwd: &Path) -> Result<()> {
        let old_thread = self.thread_id.clone();
        self.thread_id = create_session_thread_id(cwd, Some(create_tsid().as_str()));
        self.checkpoint_config = interactive_config(&self.thread_id);
        self.step = 0;
        self.saver.delete_thread(&old_thread).await?;
        self.ensure_bootstrap_checkpoint().await
    }

    /// List recent checkpoints for the active thread (newest first).
    pub async fn list_checkpoint_history(&self, limit: usize) -> Result<Vec<CheckpointSummary>> {
        let limit = limit.clamp(1, 100) as u64;
        let tuples = self
            .saver
            .list(
                &interactive_config(&self.thread_id),
                &CheckpointListOptions {
                    limit: Some(limit),
                    before: None,
                    filter: None,
                },
            )
            .await?;
        Ok(tuples
            .into_iter()
            .map(|tuple| {
                let messages = messages_from_checkpoint(&tuple.checkpoint);
                CheckpointSummary {
                    checkpoint_id: tuple.config.configurable.checkpoint_id.unwrap_or_default(),
                    step: tuple.metadata.as_ref().map(|m| m.step).unwrap_or(0),
                    source: tuple.metadata.as_ref().map(|m| m.source.clone()).unwrap_or_default(),
                    message_count: messages.len(),
                }
            })
            .collect())
    }

    /// Resolve `/restore` argument: 1-based index from [`Self::list_checkpoint_history`] or id prefix.
    pub async fn resolve_checkpoint_id(&self, arg: &str) -> Result<String> {
        let parsed_index = arg.parse::<usize>();
        if let Ok(index) = &parsed_index
            && *index == 0
        {
            anyhow::bail!("checkpoint index must be >= 1");
        }
        if let Ok(index) = parsed_index {
            let list = self.list_checkpoint_history(100).await?;
            let item = list
                .get(index - 1)
                .with_context(|| format!("checkpoint #{index} not found"))?;
            return Ok(item.checkpoint_id.clone());
        }
        let list = self.list_checkpoint_history(100).await?;
        let matches: Vec<_> = list
            .iter()
            .filter(|summary| summary.checkpoint_id.starts_with(arg))
            .collect();
        match matches.len() {
            0 => anyhow::bail!("no checkpoint matching '{arg}'"),
            1 => Ok(matches[0].checkpoint_id.clone()),
            n => anyhow::bail!("ambiguous checkpoint prefix '{arg}' ({n} matches)"),
        }
    }

    /// Load a checkpoint tuple for this thread (`checkpoint_id` = latest when `None`).
    pub async fn get_checkpoint_tuple(&self, checkpoint_id: Option<&str>) -> Result<Option<CheckpointTuple>> {
        let mut config = interactive_config(&self.thread_id);
        if let Some(id) = checkpoint_id {
            config.configurable.checkpoint_id = Some(id.to_string());
        }
        self.saver.get_tuple(&config).await
    }

    /// Reposition the session to an earlier checkpoint (fork on next turn).
    pub async fn restore_checkpoint(&mut self, checkpoint_id: &str) -> Result<usize> {
        let config = RunnableConfig {
            configurable: CheckpointConfigurable {
                thread_id: self.thread_id.clone(),
                checkpoint_ns: String::new(),
                checkpoint_id: Some(checkpoint_id.to_string()),
            },
        };
        let tuple = self
            .saver
            .get_tuple(&config)
            .await?
            .with_context(|| format!("checkpoint {checkpoint_id} not found"))?;
        let messages = messages_from_checkpoint(&tuple.checkpoint);
        let count = messages.len();
        self.step = tuple.metadata.as_ref().map(|m| m.step).unwrap_or(0);
        self.checkpoint_config = tuple.config;
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::{MESSAGES_CHANNEL, interactive_config, tool_write_channel};
    use std::path::PathBuf;
    use tempfile::TempDir;

    async fn test_session(dir: &TempDir, cwd: &Path) -> SessionStore {
        let path: PathBuf = dir.path().join("test.sqlite");
        SessionStore::open_with_path(path, cwd).await.expect("open session")
    }

    #[tokio::test]
    async fn full_turn_persists_tool_and_message_writes() {
        let dir = TempDir::new().expect("tempdir");
        let cwd = dir.path().join("repo");
        std::fs::create_dir_all(&cwd).expect("cwd");
        let mut store = test_session(&dir, &cwd).await;
        let turn_config = store.turn_write_context().config.clone();

        store
            .turn_write_context()
            .record_tool_result("call-1", "read", "{}", false, "file contents")
            .await
            .expect("tool write");

        let user = elph_agent::llm_message_to_agent(elph_ai::Message::User {
            content: elph_ai::UserContent::Text("hi".into()),
            timestamp: 0,
        });
        store
            .save_messages(std::slice::from_ref(&user), "chat")
            .await
            .expect("save");

        let turn_id = turn_config.configurable.checkpoint_id.expect("turn checkpoint id");
        let parent = store
            .get_checkpoint_tuple(Some(&turn_id))
            .await
            .expect("get")
            .expect("parent tuple");
        assert_eq!(parent.pending_writes.len(), 2);
        let channels: Vec<_> = parent.pending_writes.iter().map(|(_, ch, _)| ch.as_str()).collect();
        assert!(channels.contains(&tool_write_channel("read").as_str()));
        assert!(channels.contains(&MESSAGES_CHANNEL));
    }

    #[tokio::test]
    async fn reset_thread_bootstraps_new_checkpoint() {
        let dir = TempDir::new().expect("tempdir");
        let cwd = dir.path().join("repo");
        std::fs::create_dir_all(&cwd).expect("cwd");
        let mut store = test_session(&dir, &cwd).await;
        let first_id = store
            .checkpoint_config
            .configurable
            .checkpoint_id
            .clone()
            .expect("bootstrap id");

        store.reset_thread(&cwd).await.expect("reset");
        let second_id = store
            .checkpoint_config
            .configurable
            .checkpoint_id
            .clone()
            .expect("new bootstrap id");
        assert_ne!(first_id, second_id);
        assert_eq!(store.step, 0);
    }

    #[tokio::test]
    async fn restore_checkpoint_repositions_session() {
        let dir = TempDir::new().expect("tempdir");
        let cwd = dir.path().join("repo");
        std::fs::create_dir_all(&cwd).expect("cwd");
        let mut store = test_session(&dir, &cwd).await;

        let user = elph_agent::llm_message_to_agent(elph_ai::Message::User {
            content: elph_ai::UserContent::Text("one".into()),
            timestamp: 0,
        });
        store
            .save_messages(std::slice::from_ref(&user), "chat")
            .await
            .expect("save1");
        let first_id = store
            .checkpoint_config
            .configurable
            .checkpoint_id
            .clone()
            .expect("first");

        let user2 = elph_agent::llm_message_to_agent(elph_ai::Message::User {
            content: elph_ai::UserContent::Text("two".into()),
            timestamp: 0,
        });
        store.save_messages(&[user, user2], "chat").await.expect("save2");
        assert_eq!(store.load_messages().await.expect("load").len(), 2);

        let restored = store.restore_checkpoint(&first_id).await.expect("restore");
        assert_eq!(restored, 1);
        assert_eq!(store.load_messages().await.expect("reload").len(), 1);
        assert_eq!(
            store.checkpoint_config.configurable.checkpoint_id.as_deref(),
            Some(first_id.as_str())
        );
    }

    #[tokio::test]
    async fn reset_thread_deletes_previous_thread_data() {
        let dir = TempDir::new().expect("tempdir");
        let cwd = dir.path().join("repo");
        std::fs::create_dir_all(&cwd).expect("cwd");
        let mut store = test_session(&dir, &cwd).await;
        let old_thread = store.thread_id().to_string();
        let user = elph_agent::llm_message_to_agent(elph_ai::Message::User {
            content: elph_ai::UserContent::Text("hello".into()),
            timestamp: 0,
        });
        store
            .save_messages(std::slice::from_ref(&user), "chat")
            .await
            .expect("save");

        let db_path = store.db_path().to_path_buf();
        store.reset_thread(&cwd).await.expect("reset");
        assert!(store.get_checkpoint_tuple(None).await.expect("latest").is_some());
        let old_saver = TursoCheckpointSaver::open(Some(db_path)).await.expect("reopen");
        let old = interactive_config(&old_thread);
        assert!(old_saver.get_tuple(&old).await.expect("old lookup").is_none());
    }
}
