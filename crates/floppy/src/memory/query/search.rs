use anyhow::Result;

use crate::memory::store::MemoryStore;

impl MemoryStore {
    /// Semantic search via full task lifecycle (creates a task record).
    ///
    /// Returns memories plus related past tasks (see [`StartTaskResult::related_tasks`]).
    pub async fn search(&self, query: &str) -> Result<super::super::types::StartTaskResult> {
        self.start_task(query).await
    }
}
