//! In-memory registry of active subagents.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::Mutex;

use super::harness::SubagentHarness;
use super::types::{SubagentInfo, SubagentStatus};

pub struct SubagentRecord {
    pub info: SubagentInfo,
    pub harness: Arc<SubagentHarness>,
}

pub struct AgentRegistry {
    agents: Mutex<HashMap<String, SubagentRecord>>,
    paths: Mutex<HashMap<String, String>>,
}

impl Default for AgentRegistry {
    fn default() -> Self {
        Self {
            agents: Mutex::new(HashMap::new()),
            paths: Mutex::new(HashMap::new()),
        }
    }
}

impl AgentRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn reserve_path(&self, agent_path: &str) -> Result<(), String> {
        let mut paths = self.paths.lock().await;
        if paths.contains_key(agent_path) {
            return Err(format!("Agent path already exists: {agent_path}"));
        }
        paths.insert(agent_path.to_string(), String::new());
        Ok(())
    }

    pub async fn release_path(&self, agent_path: &str) {
        self.paths.lock().await.remove(agent_path);
    }

    pub async fn commit_path(&self, agent_path: &str, agent_id: &str) {
        self.paths
            .lock()
            .await
            .insert(agent_path.to_string(), agent_id.to_string());
    }

    pub async fn insert(&self, record: SubagentRecord) {
        let agent_path = record.info.agent_path.clone();
        let agent_id = record.info.id.clone();
        self.commit_path(&agent_path, &agent_id).await;
        self.agents.lock().await.insert(agent_id, record);
    }

    pub async fn get(&self, id: &str) -> Option<SubagentRecord> {
        self.agents.lock().await.get(id).cloned()
    }

    pub async fn get_by_path(&self, agent_path: &str) -> Option<SubagentRecord> {
        let id = self.paths.lock().await.get(agent_path)?.clone();
        self.get(&id).await
    }

    pub async fn list(&self, path_prefix: Option<&str>) -> Vec<SubagentInfo> {
        self.agents
            .lock()
            .await
            .values()
            .map(|r| r.info.clone())
            .filter(|info| {
                path_prefix.is_none_or(|prefix| {
                    info.agent_path == prefix || info.agent_path.starts_with(&format!("{prefix}/"))
                })
            })
            .collect()
    }

    pub async fn set_status(&self, id: &str, status: SubagentStatus) -> bool {
        if let Some(record) = self.agents.lock().await.get_mut(id) {
            record.info.status = status;
            true
        } else {
            false
        }
    }

    pub async fn count_active(&self) -> usize {
        self.agents
            .lock()
            .await
            .values()
            .filter(|r| matches!(r.info.status, SubagentStatus::Pending | SubagentStatus::Running))
            .count()
    }

    pub async fn remove(&self, id: &str) -> Option<SubagentRecord> {
        let record = self.agents.lock().await.remove(id)?;
        self.paths.lock().await.remove(&record.info.agent_path);
        Some(record)
    }
}

impl Clone for SubagentRecord {
    fn clone(&self) -> Self {
        Self {
            info: self.info.clone(),
            harness: self.harness.clone(),
        }
    }
}
