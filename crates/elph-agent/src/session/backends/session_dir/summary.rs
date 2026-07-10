//! `summary.json` schema.

use serde::{Deserialize, Serialize};

use super::layout::CHAT_FORMAT_VERSION;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionSummary {
    pub info: SessionSummaryInfo,
    pub created_at: String,
    pub updated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_session_id: Option<String>,
    #[serde(default = "default_chat_format_version")]
    pub chat_format_version: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionSummaryInfo {
    pub id: String,
    pub cwd: String,
}

impl SessionSummary {
    pub fn new(
        id: impl Into<String>,
        cwd: impl Into<String>,
        created_at: impl Into<String>,
        parent_session_id: Option<String>,
    ) -> Self {
        let created_at = created_at.into();
        Self {
            info: SessionSummaryInfo {
                id: id.into(),
                cwd: cwd.into(),
            },
            created_at: created_at.clone(),
            updated_at: created_at,
            parent_session_id,
            chat_format_version: CHAT_FORMAT_VERSION,
        }
    }

    pub fn touch(&mut self, updated_at: impl Into<String>) {
        self.updated_at = updated_at.into();
    }
}

fn default_chat_format_version() -> u32 {
    CHAT_FORMAT_VERSION
}
