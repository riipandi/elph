use serde::{Deserialize, Serialize};

use super::super::util::DEFAULT_EMBEDDING_DIMS;

/// Turso vector type for distance calculations. Easy to swap for experimentation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VectorType {
    #[default]
    Vector32,
    Vector64,
    Vector8,
    Vector1,
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
pub struct FloppyConfig {
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
    /// Run floppy migrations in [`MemoryStore::init`] (default: true). Set `false` when the host
    /// already applied [`crate::floppy::migrations::MIGRATIONS`].
    pub apply_migrations: Option<bool>,
}

impl FloppyConfig {
    pub fn new(db_path: impl Into<String>, session_id: impl Into<String>) -> Self {
        Self {
            db_path: db_path.into(),
            session_id: session_id.into(),
            vector_type: None,
            dimensions: Some(DEFAULT_EMBEDDING_DIMS),
            top_k: None,
            learning_rate: None,
            decay_rate: None,
            apply_migrations: None,
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

    pub fn apply_migrations(mut self, apply: bool) -> Self {
        self.apply_migrations = Some(apply);
        self
    }
}
