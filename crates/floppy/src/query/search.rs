use anyhow::Result;

use super::super::store::MemoryStore;

impl MemoryStore {
    /// Semantic search via full task lifecycle (creates a task record).
    pub async fn search(&self, query: &str) -> Result<super::super::types::StartTaskResult> {
        self.start_task(query).await
    }
}
