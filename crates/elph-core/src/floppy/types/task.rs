use serde::{Deserialize, Serialize};

use super::config::MemoryCategory;

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
pub struct StartTaskResult {
    pub task_id: String,
    pub memories: Vec<super::memory::Memory>,
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
pub struct EndTaskWithDecayResult {
    pub decay: super::memory::DecayResult,
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
