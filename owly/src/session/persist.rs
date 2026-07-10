use anyhow::Result;
use elph_agent::AgentMessage;
use elph_agent::create_tsid;
use serde_json::Value;
use std::collections::HashMap;

use crate::checkpoint::{Checkpoint, CheckpointMetadata, PendingWrite, RunnableConfig, TursoCheckpointSaver};

use super::MESSAGES_CHANNEL;

/// Persist the full conversation after a completed turn.
pub async fn save_messages(
    saver: &TursoCheckpointSaver,
    config: &RunnableConfig,
    messages: &[AgentMessage],
    step: i64,
    source: &str,
) -> Result<RunnableConfig> {
    let mut checkpoint = copy_checkpoint_for_save(config, saver).await?;
    checkpoint
        .channel_values
        .insert(MESSAGES_CHANNEL.to_string(), serde_json::to_value(messages)?);
    bump_channel_version(&mut checkpoint, MESSAGES_CHANNEL);

    let metadata = CheckpointMetadata {
        source: source.to_string(),
        step,
        parents: HashMap::new(),
    };

    // LangGraph contract: pending writes attach to the checkpoint being executed
    // (current `config.checkpoint_id`), then `put` creates the next checkpoint.
    if config.configurable.checkpoint_id.is_some() {
        persist_channel_writes(saver, config, messages, source).await?;
    }

    saver.put(config, &checkpoint, &metadata).await
}

/// Mirror LangGraph pending writes into the `writes` table for the active checkpoint.
pub async fn persist_channel_writes(
    saver: &TursoCheckpointSaver,
    config: &RunnableConfig,
    messages: &[AgentMessage],
    task_id: &str,
) -> Result<()> {
    let message_value = serde_json::to_value(messages)?;
    let writes: Vec<PendingWrite> = vec![(MESSAGES_CHANNEL.to_string(), message_value)];
    saver.put_writes(config, &writes, task_id).await?;
    Ok(())
}

async fn copy_checkpoint_for_save(config: &RunnableConfig, saver: &TursoCheckpointSaver) -> Result<Checkpoint> {
    if let Some(tuple) = saver.get_tuple(config).await? {
        let mut checkpoint = crate::checkpoint::copy_checkpoint(&tuple.checkpoint);
        checkpoint.id = create_tsid();
        checkpoint.ts = chrono::Utc::now().to_rfc3339();
        return Ok(checkpoint);
    }

    Ok(Checkpoint::default())
}

fn bump_channel_version(checkpoint: &mut Checkpoint, channel: &str) {
    let next = checkpoint
        .channel_versions
        .get(channel)
        .and_then(|v| v.parse::<i64>().ok())
        .map(|v| (v + 1).to_string())
        .unwrap_or_else(|| "1".to_string());
    checkpoint.channel_versions.insert(channel.to_string(), next);
}

pub fn messages_from_checkpoint(checkpoint: &Checkpoint) -> Vec<AgentMessage> {
    checkpoint
        .channel_values
        .get(MESSAGES_CHANNEL)
        .and_then(|value| serde_json::from_value::<Vec<AgentMessage>>(value.clone()).ok())
        .unwrap_or_default()
}

pub fn messages_to_channel_value(messages: &[AgentMessage]) -> Result<Value> {
    Ok(serde_json::to_value(messages)?)
}
