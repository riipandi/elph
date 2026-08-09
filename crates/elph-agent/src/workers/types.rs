//! Shared worker-domain types.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkerStatus {
    Online,
    Idle,
    Busy,
    Stale,
    Offline,
}

impl WorkerStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Online => "online",
            Self::Idle => "idle",
            Self::Busy => "busy",
            Self::Stale => "stale",
            Self::Offline => "offline",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "online" => Some(Self::Online),
            "idle" => Some(Self::Idle),
            "busy" => Some(Self::Busy),
            "stale" => Some(Self::Stale),
            "offline" => Some(Self::Offline),
            _ => None,
        }
    }

    pub fn is_live(self) -> bool {
        matches!(self, Self::Online | Self::Idle | Self::Busy)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkerRecord {
    pub worker_id: String,
    pub session_id: String,
    pub project_key: String,
    pub name: String,
    pub purpose: String,
    pub model: Option<String>,
    pub status: WorkerStatus,
    pub context_pct: Option<f64>,
    pub pid: Option<i64>,
    pub hostname: Option<String>,
    pub started_at: String,
    pub heartbeat_at: String,
}

/// Peer summary for tools / TUI (excludes heavy fields).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LiveWorker {
    pub worker_id: String,
    pub session_id: String,
    pub name: String,
    pub purpose: String,
    pub model: Option<String>,
    pub status: WorkerStatus,
    pub context_pct: Option<f64>,
    pub is_self: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageKind {
    Prompt,
    Response,
    Notify,
}

impl MessageKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Prompt => "prompt",
            Self::Response => "response",
            Self::Notify => "notify",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "prompt" => Some(Self::Prompt),
            "response" => Some(Self::Response),
            "notify" => Some(Self::Notify),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageStatus {
    Queued,
    Delivered,
    Complete,
    Error,
    Timeout,
}

impl MessageStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Delivered => "delivered",
            Self::Complete => "complete",
            Self::Error => "error",
            Self::Timeout => "timeout",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "queued" => Some(Self::Queued),
            "delivered" => Some(Self::Delivered),
            "complete" => Some(Self::Complete),
            "error" => Some(Self::Error),
            "timeout" => Some(Self::Timeout),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkerMessage {
    pub id: String,
    pub project_key: String,
    pub from_worker_id: String,
    pub from_session_id: String,
    pub to_worker_id: Option<String>,
    pub to_session_id: String,
    pub kind: MessageKind,
    pub status: MessageStatus,
    pub conversation_id: Option<String>,
    pub parent_msg_id: Option<String>,
    pub hops: i64,
    pub payload: String,
    pub created_at: String,
    pub delivered_at: Option<String>,
    pub completed_at: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FileLease {
    pub project_key: String,
    pub path_norm: String,
    pub worker_id: String,
    pub session_id: String,
    pub mode: String,
    pub purpose: Option<String>,
    pub content_hash: Option<String>,
    pub acquired_at: String,
    pub heartbeat_at: String,
}
