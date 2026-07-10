use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    pub v: u8,
    pub id: String,
    pub ts: String,
    #[serde(default)]
    pub channel_values: HashMap<String, Value>,
    #[serde(default)]
    pub channel_versions: HashMap<String, String>,
    #[serde(default)]
    pub versions_seen: HashMap<String, HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointMetadata {
    pub source: String,
    pub step: i64,
    #[serde(default)]
    pub parents: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunnableConfig {
    pub configurable: CheckpointConfigurable,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointConfigurable {
    pub thread_id: String,
    #[serde(default)]
    pub checkpoint_ns: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checkpoint_id: Option<String>,
}

pub type PendingWrite = (String, Value);
pub type CheckpointPendingWrite = (String, String, Value);

#[derive(Debug, Clone)]
pub struct CheckpointTuple {
    pub config: RunnableConfig,
    pub checkpoint: Checkpoint,
    pub metadata: Option<CheckpointMetadata>,
    pub parent_config: Option<RunnableConfig>,
    pub pending_writes: Vec<CheckpointPendingWrite>,
}

#[derive(Debug, Clone, Default)]
pub struct CheckpointListOptions {
    pub limit: Option<u64>,
    pub before: Option<RunnableConfig>,
    pub filter: Option<HashMap<String, Value>>,
}

impl Default for Checkpoint {
    fn default() -> Self {
        Self {
            v: 4,
            id: elph_agent::create_tsid(),
            ts: chrono::Utc::now().to_rfc3339(),
            channel_values: HashMap::new(),
            channel_versions: HashMap::new(),
            versions_seen: HashMap::new(),
        }
    }
}
