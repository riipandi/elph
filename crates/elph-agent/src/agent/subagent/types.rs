//! Subagent types.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::agent::harness::{AgentHarnessResources, AgentHarnessStreamOptions};
use crate::agent::subagent::graph::AgentGraphStore;
use crate::prompt::encoding::PromptEncodingConfig;
use crate::types::AgentThinkingLevel;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubagentStatus {
    Pending,
    Running,
    Idle,
    Error,
    Done,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SubagentOutput {
    /// Final assistant text of the last completed turn, trimmed. Empty when the
    /// subagent has not produced a final message yet.
    pub text: String,
    /// Absolute path to the persistent output log (`output.md`). `None` when no
    /// outputs dir is configured for the session.
    pub output_path: Option<String>,
    /// Epoch milliseconds of the last completed turn.
    pub finished_at_ms: Option<i64>,
    /// Number of assistant turns completed (initial spawn + follow-ups).
    pub turns: u32,
}

impl SubagentOutput {
    /// Non-empty human-readable summary for tool results / UI. Falls back to a
    /// stable placeholder instead of an empty string so callers never produce a
    /// blank "no output" result.
    pub fn summary(&self) -> String {
        let text = self.text.trim();
        if text.is_empty() {
            if let Some(path) = self.output_path.as_deref().filter(|p| !p.is_empty()) {
                format!("(no text output — full log: {path})")
            } else {
                "(no output captured)".to_string()
            }
        } else {
            text.to_string()
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubagentInfo {
    pub id: String,
    pub session_id: String,
    pub task_name: String,
    pub agent_path: String,
    pub depth: u32,
    pub status: SubagentStatus,
    pub parent_session_id: String,
    /// Model the subagent runs with, formatted `provider_id/model_id`.
    pub model: String,
    /// Output of the last completed turn (persisted + traced).
    pub output: SubagentOutput,
}

#[derive(Debug, Clone)]
pub struct SubagentLimits {
    pub max_depth: u32,
    pub max_concurrent: usize,
}

impl Default for SubagentLimits {
    fn default() -> Self {
        Self {
            max_depth: 3,
            max_concurrent: 4,
        }
    }
}

/// Shared bootstrap data for spawning child session harnesses.
#[derive(Clone)]
pub struct SubagentBootstrap {
    pub cwd: String,
    /// Shared project database (`.elph/store.db` in the product).
    pub store_db_path: String,
    pub resources: AgentHarnessResources,
    pub stream_options: AgentHarnessStreamOptions,
    pub thinking_level: AgentThinkingLevel,
    /// TOON prompt-encoding config inherited from the parent; `None` falls back to env.
    pub prompt_encoding: Option<PromptEncodingConfig>,
    pub agent_graph: Option<Arc<AgentGraphStore>>,
    /// Shared, already-open database handle. When present, child repos connect
    /// from this handle instead of opening [`store_db_path`] themselves.
    pub database: Option<Arc<turso::Database>>,
    /// Session artifacts root shared by every subagent
    /// (`APP_DATA/sessions/<SESSION_ID>` in the product). When set, each spawned
    /// agent writes a persistent, traceable log under
    /// `outputs_root/subagents/<agent_id>/`:
    ///
    /// - `output.md`: final assistant text (re-written per completed turn)
    /// - `events.jsonl`: streamed output deltas (append-only, replayable)
    /// - `meta.json`: spawn metadata (agent id, task, path, depth, session ids)
    pub outputs_root: Option<PathBuf>,
}

impl SubagentBootstrap {
    /// Directory storing this subagent's persistent artifacts:
    /// `outputs_root/subagents/<agent_id>`.
    pub fn output_dir_for(&self, agent_id: &str) -> Option<PathBuf> {
        self.outputs_root
            .as_ref()
            .map(|root| root.join("subagents").join(agent_id))
    }

    /// Create (and populate) the persistent output directory for a subagent.
    pub fn ensure_output_dir(&self, agent_id: &str) -> Option<PathBuf> {
        let dir = self.output_dir_for(agent_id)?;
        let _ = std::fs::create_dir_all(&dir);
        Some(dir)
    }
}

/// Persistent per-subagent artifact layout helpers.
pub mod persist {
    use super::*;

    pub const OUTPUT_MD: &str = "output.md";
    pub const EVENTS_JSONL: &str = "events.jsonl";
    pub const META_JSON: &str = "meta.json";

    /// Write (or rewrite) the final assistant text of a completed turn.
    pub fn write_output(dir: &Path, text: &str) {
        let _ = std::fs::write(dir.join(OUTPUT_MD), text);
    }

    /// Append a streamed output delta to `events.jsonl` (best-effort).
    pub fn append_event(dir: &Path, event: &str, text: &str) {
        let ts = crate::messages::now_iso_timestamp();
        let line = format!(
            "{{\"event\":{},\"ts\":{},\"text\":{}}}",
            serde_json::to_string(event).unwrap_or_else(|_| "\"unknown\"".into()),
            serde_json::to_string(&ts).unwrap_or_else(|_| "\"1970-01-01T00:00:00Z\"".into()),
            serde_json::to_string(text).unwrap_or_else(|_| "\"\"".into()),
        );
        use std::io::Write;
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join(EVENTS_JSONL))
        {
            let _ = writeln!(file, "{line}");
        }
    }

    /// Write spawn metadata (rewritten once per spawn).
    pub fn write_meta(dir: &Path, info: &SubagentInfo) {
        if let Ok(json) = serde_json::to_string_pretty(info) {
            let _ = std::fs::write(dir.join(META_JSON), json);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_summary_returns_text_when_present() {
        let output = SubagentOutput {
            text: "Review complete.".into(),
            output_path: Some("/tmp/x/subagents/a/output.md".into()),
            finished_at_ms: Some(1),
            turns: 2,
        };
        assert_eq!(output.summary(), "Review complete.");
    }

    #[test]
    fn output_summary_falls_back_to_log_path() {
        let output = SubagentOutput {
            text: String::new(),
            output_path: Some("/tmp/x/subagents/a/output.md".into()),
            finished_at_ms: None,
            turns: 0,
        };
        assert_eq!(output.summary(), "(no text output — full log: /tmp/x/subagents/a/output.md)");
    }

    #[test]
    fn output_summary_falls_back_to_placeholder() {
        let output = SubagentOutput {
            text: "  ".into(),
            output_path: None,
            finished_at_ms: None,
            turns: 0,
        };
        assert_eq!(output.summary(), "(no output captured)");
    }

    #[test]
    fn bootstrap_output_dir_is_namespaced_by_agent() {
        let bootstrap = SubagentBootstrap {
            cwd: "/tmp".into(),
            store_db_path: "/tmp/db".into(),
            resources: crate::agent::harness::AgentHarnessResources::default(),
            stream_options: crate::agent::harness::AgentHarnessStreamOptions::default(),
            thinking_level: Default::default(),
            prompt_encoding: None,
            agent_graph: None,
            database: None,
            outputs_root: Some(std::path::PathBuf::from("/data/sessions/s1")),
        };
        let dir = bootstrap.output_dir_for("agent_abc").expect("dir");
        assert_eq!(dir.to_string_lossy(), "/data/sessions/s1/subagents/agent_abc");
    }
}
