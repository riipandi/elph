use serde::{Deserialize, Serialize};

use super::util::DEFAULT_EMBEDDING_DIMS;

/// Turso vector type for distance calculations. Easy to swap for experimentation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VectorType {
    Vector32,
    Vector64,
    Vector8,
    Vector1,
}

impl Default for VectorType {
    fn default() -> Self {
        VectorType::Vector32
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryCategory {
    Correction,
    Insight,
    User,
    Consolidated,
    Discovery,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UserInputSource {
    UserDenial,
    UserCorrection,
    UserInput,
}

#[derive(Debug, Clone)]
pub struct MemzConfig {
    /// Path to the Turso database file
    pub db_path: String,
    /// Session identifier — each agent session gets its own ID
    pub session_id: String,
    /// Vector type for distance calculations (default: Vector32)
    pub vector_type: Option<VectorType>,
    /// Embedding dimensions (default: 384)
    pub dimensions: Option<u32>,
    /// Number of memories to retrieve per task (default: 5)
    pub top_k: Option<u32>,
    /// EMA learning rate for weight updates (default: 0.1)
    pub learning_rate: Option<f64>,
    /// Daily decay rate for unused memories (default: 0.995)
    pub decay_rate: Option<f64>,
}

impl MemzConfig {
    pub fn new(db_path: impl Into<String>, session_id: impl Into<String>) -> Self {
        Self {
            db_path: db_path.into(),
            session_id: session_id.into(),
            vector_type: None,
            dimensions: Some(DEFAULT_EMBEDDING_DIMS),
            top_k: None,
            learning_rate: None,
            decay_rate: None,
        }
    }

    pub fn vector_type(mut self, vector_type: VectorType) -> Self {
        self.vector_type = Some(vector_type);
        self
    }

    pub fn dimensions(mut self, dimensions: u32) -> Self {
        self.dimensions = Some(dimensions);
        self
    }

    pub fn top_k(mut self, top_k: u32) -> Self {
        self.top_k = Some(top_k);
        self
    }

    pub fn learning_rate(mut self, learning_rate: f64) -> Self {
        self.learning_rate = Some(learning_rate);
        self
    }

    pub fn decay_rate(mut self, decay_rate: f64) -> Self {
        self.decay_rate = Some(decay_rate);
        self
    }
}

/// Unified memory report input.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryReportType {
    Correction,
    UserInput,
    Insight,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryReportInput {
    #[serde(rename = "type")]
    pub report_type: MemoryReportType,
    pub lesson: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub what_failed: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub what_worked: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tokens_wasted: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools_wasted: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<UserInputSource>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EmbeddingStatus {
    Ok,
    Pending,
    Truncated,
}

/// Full memory row for inspection APIs (`elph memory list`).
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

/// Extended status (`elph memory status`).
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    InProgress,
    Completed,
    Failed,
}

/// Task summary (`tasks` CLI command).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRecord {
    pub id: String,
    pub description: Option<String>,
    pub tokens_used: Option<u32>,
    pub tool_calls: Option<u32>,
    pub errors: Option<u32>,
    pub user_corrections: Option<u32>,
    pub status: TaskStatus,
    pub task_score: Option<f64>,
    pub started_at: Option<i64>,
    pub finished_at: Option<i64>,
    pub retrievals: Vec<TaskRetrieval>,
    pub created_memories: Vec<TaskCreatedMemory>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRetrieval {
    pub memory_id: String,
    pub category: MemoryCategory,
    pub preview: String,
    pub similarity: Option<f64>,
    pub self_report: Option<u8>,
    pub credit: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskCreatedMemory {
    pub category: MemoryCategory,
    pub preview: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimelineEventKind {
    Task,
    Memory,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineEvent {
    pub timestamp: i64,
    pub kind: TimelineEventKind,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContradictResult {
    pub deleted: bool,
    pub correction_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EndTaskWithDecayResult {
    pub decay: DecayResult,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartTaskResult {
    pub task_id: String,
    pub memories: Vec<Memory>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportCorrectionInput {
    /// The lesson learned
    pub lesson: String,
    /// What approach failed
    pub what_failed: String,
    /// What approach worked
    pub what_worked: String,
    /// Approximate tokens spent on the wrong approach
    pub tokens_wasted: Option<u32>,
    /// Number of tool calls wasted on the wrong approach
    pub tools_wasted: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportUserInput {
    /// The lesson / knowledge from the user
    pub lesson: String,
    /// How the user provided this
    pub source: UserInputSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelfReportEntry {
    pub memory_id: String,
    /// 0 = ignored, 1 = glanced, 2 = somewhat useful, 3 = directly applied
    pub score: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskEndInput {
    pub tokens_used: u32,
    pub tool_calls: u32,
    pub errors: u32,
    pub user_corrections: u32,
    pub completed: bool,
    pub self_report: Option<Vec<SelfReportEntry>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopMemory {
    pub content: String,
    pub weight: f64,
    pub retrieval_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryStats {
    pub total_memories: u32,
    pub task_count: u32,
    pub avg_task_score: f64,
    pub top_memories: Vec<TopMemory>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DecayResult {
    pub decayed: u32,
    pub deleted: u32,
}

/// Running baseline for z-score computation
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskBaseline {
    pub count: u32,
    pub mean_tokens: f64,
    pub mean_errors: f64,
    pub mean_user_corrections: f64,
    pub m2_tokens: f64,
    pub m2_errors: f64,
    pub m2_user_corrections: f64,
}
