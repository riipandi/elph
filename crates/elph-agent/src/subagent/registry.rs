//! In-memory registry of active subagents.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::Mutex;

use super::types::{SubagentInfo, SubagentStatus};
use crate::agent::Agent;

pub struct SubagentRecord {
    pub info: SubagentInfo,
    pub agent: Arc<Agent>,
}

pub struct AgentRegistry {
    agents: Mutex<HashMap<String, SubagentRecord>>,
}

impl Default for AgentRegistry {
    fn default() -> Self {
        Self {
            agents: Mutex::new(HashMap::new()),
        }
    }
}

impl AgentRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn insert(&self, record: SubagentRecord) {
        self.agents.lock().await.insert(record.info.id.clone(), record);
    }

    pub async fn get(&self, id: &str) -> Option<SubagentRecord> {
        self.agents.lock().await.get(id).cloned()
    }

    pub async fn list(&self) -> Vec<SubagentInfo> {
        self.agents.lock().await.values().map(|r| r.info.clone()).collect()
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
        self.agents.lock().await.remove(id)
    }
}

impl Clone for SubagentRecord {
    fn clone(&self) -> Self {
        Self {
            info: self.info.clone(),
            agent: self.agent.clone(),
        }
    }
}
