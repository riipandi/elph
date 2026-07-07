use serde::{Deserialize, Serialize};

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
