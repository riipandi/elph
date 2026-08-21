//! Harness configuration and resource types.

use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;

use elph_ai::{ImageContent, Model, Models, Transport};
use serde_json::Value;

use crate::runtime::local_env::LocalExecutionEnv;
use crate::session::Session;
use crate::types::{AgentThinkingLevel, AgentTool, QueueMode};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub content: String,
    pub file_path: String,
    pub disable_model_invocation: bool,
    /// License name or reference to a bundled license file.
    pub license: Option<String>,
    /// Environment requirements (intended product, system packages, etc.). Max 500 chars.
    pub compatibility: Option<String>,
    /// Arbitrary key-value mapping for additional metadata.
    pub metadata: Option<std::collections::HashMap<String, Value>>,
    /// Space-separated list of pre-approved tools the skill may use.
    pub allowed_tools: Option<Vec<String>>,
    /// Palette / slash hint; `<name>` marks required args, `[name]` optional (prompt-template convention).
    pub argument_hint: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptTemplate {
    pub name: String,
    pub description: String,
    pub content: String,
    /// Optional argument hint shown in the slash palette (e.g. `"<PR-URL>"`).
    pub argument_hint: Option<String>,
    /// Source file path (used for `resources.prompts` `!` / `-` exclude matching).
    pub file_path: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AgentHarnessResources {
    pub prompt_templates: Vec<PromptTemplate>,
    pub skills: Vec<Skill>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AgentHarnessStreamOptions {
    pub transport: Option<Transport>,
    pub timeout_ms: Option<u64>,
    pub max_retries: Option<u32>,
    pub max_retry_delay_ms: Option<u64>,
    pub headers: Option<std::collections::HashMap<String, String>>,
    pub metadata: Option<Value>,
    pub cache_retention: Option<String>,
    pub thinking_budgets: Option<elph_ai::ThinkingBudgets>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AgentHarnessStreamOptionsPatch {
    pub transport: Option<Transport>,
    /// `None` = leave unchanged, `Some(None)` = clear, `Some(Some(v))` = set.
    pub timeout_ms: Option<Option<u64>>,
    /// `None` = leave unchanged, `Some(None)` = clear, `Some(Some(v))` = set.
    pub max_retries: Option<Option<u32>>,
    /// `None` = leave unchanged, `Some(None)` = clear, `Some(Some(v))` = set.
    pub max_retry_delay_ms: Option<Option<u64>>,
    /// `None` = leave unchanged, `Some(None)` = clear all headers, `Some(Some(map))` = merge/delete keys.
    pub headers: Option<Option<std::collections::HashMap<String, Option<String>>>>,
    /// `None` = leave unchanged, `Some(None)` = clear all metadata, `Some(Some(map))` = merge/delete keys.
    pub metadata: Option<Option<std::collections::HashMap<String, Option<Value>>>>,
    pub cache_retention: Option<String>,
}

pub fn clone_stream_options(stream_options: &AgentHarnessStreamOptions) -> AgentHarnessStreamOptions {
    AgentHarnessStreamOptions {
        transport: stream_options.transport,
        timeout_ms: stream_options.timeout_ms,
        max_retries: stream_options.max_retries,
        max_retry_delay_ms: stream_options.max_retry_delay_ms,
        headers: stream_options.headers.clone(),
        metadata: stream_options.metadata.clone(),
        cache_retention: stream_options.cache_retention.clone(),
        thinking_budgets: stream_options.thinking_budgets.clone(),
    }
}

pub fn apply_stream_options_patch(
    base: AgentHarnessStreamOptions,
    patch: &AgentHarnessStreamOptionsPatch,
) -> AgentHarnessStreamOptions {
    let mut result = clone_stream_options(&base);
    if patch.transport.is_some() {
        result.transport = patch.transport;
    }
    if let Some(timeout_ms) = patch.timeout_ms {
        result.timeout_ms = timeout_ms;
    }
    if let Some(max_retries) = patch.max_retries {
        result.max_retries = max_retries;
    }
    if let Some(max_retry_delay_ms) = patch.max_retry_delay_ms {
        result.max_retry_delay_ms = max_retry_delay_ms;
    }
    if patch.cache_retention.is_some() {
        result.cache_retention = patch.cache_retention.clone();
    }
    if let Some(headers_patch) = &patch.headers {
        result.headers = match headers_patch {
            None => None,
            Some(map) => {
                let mut headers = result.headers.take().unwrap_or_default();
                for (key, value) in map {
                    match value {
                        Some(value) => {
                            headers.insert(key.clone(), value.clone());
                        }
                        None => {
                            headers.remove(key);
                        }
                    }
                }
                if headers.is_empty() { None } else { Some(headers) }
            }
        };
    }
    if let Some(metadata_patch) = &patch.metadata {
        result.metadata = match metadata_patch {
            None => None,
            Some(map) => {
                let mut metadata = result
                    .metadata
                    .as_ref()
                    .and_then(|value| value.as_object())
                    .cloned()
                    .unwrap_or_default();
                for (key, value) in map {
                    match value {
                        Some(value) => {
                            metadata.insert(key.clone(), value.clone());
                        }
                        None => {
                            metadata.remove(key);
                        }
                    }
                }
                if metadata.is_empty() {
                    None
                } else {
                    Some(serde_json::Value::Object(metadata))
                }
            }
        };
    }
    result
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompactionSettings {
    pub enabled: bool,
    pub reserve_tokens: u64,
    /// Context-window usage percentage that triggers compaction.
    /// When `Some(pct)`, compaction fires when `context > context_window * pct / 100`.
    /// When `None`, falls back to `reserve_tokens` (legacy behavior).
    pub threshold_pct: Option<u8>,
    pub keep_recent_tokens: u64,
    /// When true, physically DELETE `session_entries` not on the post-compaction branch.
    pub physical_prune: bool,
}

impl Default for CompactionSettings {
    fn default() -> Self {
        DEFAULT_COMPACTION_SETTINGS
    }
}

pub const DEFAULT_COMPACTION_SETTINGS: CompactionSettings = CompactionSettings {
    enabled: true,
    reserve_tokens: 16384,
    threshold_pct: Some(80),
    keep_recent_tokens: 20000,
    physical_prune: true,
};

/// Validation settings for skill loading.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SkillValidationSettings {
    /// Emit diagnostics for optional field violations (e.g. compatibility > 500 chars).
    pub strict_mode: bool,
}

pub const DEFAULT_SKILL_VALIDATION_SETTINGS: SkillValidationSettings = SkillValidationSettings { strict_mode: false };

/// Options for loading skills from directories.
#[derive(Debug, Clone, Default)]
pub struct SkillLoadOptions {
    /// Validation settings for skill loading.
    pub validation: SkillValidationSettings,
}

/// Explicit skill discovery paths supplied by the host application.
#[derive(Debug, Clone, Default)]
pub struct SkillsDiscoveryPaths {
    pub user_dirs: Vec<String>,
    pub project_dirs: Vec<String>,
}

impl SkillsDiscoveryPaths {
    pub fn all_dirs(&self) -> Vec<String> {
        let mut dirs = self.user_dirs.clone();
        dirs.extend(self.project_dirs.clone());
        dirs
    }
}

/// Resolve user-level skill directories based on app name.
///
/// Prefer [`SkillsDiscoveryPaths`] with host-specific paths in new integrations.
/// Returns: `["~/.agents/skills", "~/.{app_name}/skills", "~/.{app_name}/bundled/skills"]`
pub fn resolve_user_skills_dirs(app_name: &str) -> Vec<String> {
    let home = std::env::var("HOME").unwrap_or_else(|_| "~".to_string());
    let mut dirs = vec![format!("{home}/.agents/skills"), format!("{home}/.{app_name}/skills")];
    let bundled = format!("{home}/.{app_name}/bundled/skills");
    dirs.push(bundled);
    dirs
}

/// Resolve project-level skill directories based on app name.
/// Returns: `["{project}/.agents/skills", "{project}/.{app_name}/skills"]`
pub fn resolve_project_skills_dirs(project_dir: &str, app_name: &str) -> Vec<String> {
    vec![
        format!("{project_dir}/.agents/skills"),
        format!("{project_dir}/.{app_name}/skills"),
    ]
}

#[derive(Debug, Clone, Default)]
pub struct AgentHarnessPromptOptions {
    pub images: Option<Vec<ImageContent>>,
}

// ---------------------------------------------------------------------------
// Agent harness options
// ---------------------------------------------------------------------------

pub struct SystemPromptContext<S: crate::session::types::SessionStorage> {
    pub env: Arc<LocalExecutionEnv>,
    pub session: Session<S>,
    pub model: Model,
    pub thinking_level: AgentThinkingLevel,
    pub active_tools: Vec<AgentTool>,
    pub resources: AgentHarnessResources,
}

pub type SystemPromptFn<S> =
    Arc<dyn Fn(SystemPromptContext<S>) -> Pin<Box<dyn Future<Output = String> + Send>> + Send + Sync>;

pub enum SystemPrompt<S: crate::session::types::SessionStorage> {
    Static(String),
    Dynamic(SystemPromptFn<S>),
}

pub struct AgentHarnessOptions<S>
where
    S: crate::session::types::SessionStorage + Send + Sync + 'static,
    S::Metadata: crate::session::types::HasSessionId + Send + Sync,
{
    pub env: Arc<LocalExecutionEnv>,
    pub session: Session<S>,
    pub models: Arc<Models>,
    pub tools: Vec<AgentTool>,
    pub resources: AgentHarnessResources,
    pub system_prompt: SystemPrompt<S>,
    pub stream_options: AgentHarnessStreamOptions,
    pub model: Model,
    pub thinking_level: AgentThinkingLevel,
    pub active_tool_names: Vec<String>,
    pub steering_mode: QueueMode,
    pub follow_up_mode: QueueMode,
    #[cfg(feature = "backend-turso")]
    pub goal_runtime: Option<std::sync::Arc<crate::goals::GoalRuntime>>,
    /// Optional relational turn accounting (`session_turns` + session rollups).
    #[cfg(feature = "backend-turso")]
    pub turn_store: Option<std::sync::Arc<crate::turns::TurnStore>>,
    pub subagent_bootstrap: Option<crate::agent::subagent::SubagentBootstrap>,
    pub compaction_settings: CompactionSettings,
    pub shared_registry: Option<std::sync::Arc<crate::agent::subagent::AgentRegistry>>,
    pub agent_control: Option<std::sync::Arc<crate::agent::subagent::AgentControl>>,
    /// Whether the harness runs in headless mode (`elph run`). Relaxes some tool
    /// defaults (e.g. no background-task timeout by default).
    pub headless: bool,
    /// Session terminal-capture directory (`APP_DATA/sessions/<SESSION_ID>/terminals`),
    /// threaded into tool contexts so tools like `shell_exec` can persist raw
    /// output. `None` for stateless contexts (tests, examples).
    pub terminals_dir: Option<PathBuf>,
}

/// Policy when restored active tool names are missing from the host registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MissingActiveToolsPolicy {
    /// Drop unknown names and continue (default).
    #[default]
    DropMissing,
    /// Fail harness construction.
    Fail,
}

/// Recovery policy for unfinished operations after process restart.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RecoveryPolicy {
    /// Mark unfinished turns interrupted; do not auto-retry tools (default).
    #[default]
    MarkInterrupted,
    /// Reserved: retry only idempotent tools (falls back to MarkInterrupted today).
    RetryUnfinished,
}

/// Options for [`crate::agent::harness::AgentHarness::restore`].
#[derive(Debug, Clone, Default)]
pub struct RestoreOptions {
    pub missing_active_tools: MissingActiveToolsPolicy,
    pub recovery: RecoveryPolicy,
}
