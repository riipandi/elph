use serde::{Deserialize, Serialize};

use super::config::UserInputSource;

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContradictResult {
    pub deleted: bool,
    pub correction_id: Option<String>,
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
