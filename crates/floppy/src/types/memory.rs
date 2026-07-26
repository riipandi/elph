use serde::{Deserialize, Serialize};

use super::config::MemoryCategory;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EmbeddingStatus {
    Ok,
    Pending,
    Truncated,
}

/// Full memory row for inspection and listing APIs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryRecord {
    pub id: String,
    pub content: String,
    pub category: MemoryCategory,
    pub weight: f64,
    pub retrieval_count: u32,
    pub created_at: i64,
    pub embedding_status: EmbeddingStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryCount {
    pub category: MemoryCategory,
    pub count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopMemory {
    pub content: String,
    pub weight: f64,
    pub retrieval_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Memory {
    pub id: String,
    pub content: String,
    pub category: MemoryCategory,
    pub weight: f64,
    /// Retrieval score: cosine similarity (0-1)
    pub score: f64,
    pub created_at: i64,
    pub retrieval_count: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DecayResult {
    pub decayed: u32,
    pub deleted: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryStats {
    pub total_memories: u32,
    pub task_count: u32,
    pub avg_task_score: f64,
    pub top_memories: Vec<TopMemory>,
}

/// Extended store status (counts, categories, top memories, task metrics).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoreStatus {
    pub total_memories: u32,
    /// Tasks with `finished_at` set.
    pub completed_tasks: u32,
    /// All tasks including in-progress.
    pub total_tasks: u32,
    pub avg_task_score: f64,
    pub categories: Vec<CategoryCount>,
    pub top_memories: Vec<TopMemory>,
}
