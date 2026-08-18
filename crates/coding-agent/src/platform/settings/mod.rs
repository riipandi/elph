//! Elph user settings: home + project layers.
//!
//! # Layers
//!
//! | Layer    | Path                              | Role                                      |
//! |----------|-----------------------------------|-------------------------------------------|
//! | Defaults | (in code)                         | Serde field defaults for missing keys     |
//! | Home     | `CONFIG_DIR/settings.json`        | Global prefs; default write target        |
//! | Project  | `<project>/.elph/settings.json`   | Per-repo overrides (always merged) |
//!
//! Runtime load always merges **home ← project** (project wins per nested object key; arrays replace).
//! `trust.defaultProjectTrust` / `trust.json` only gate **project WASM extensions**, not settings or skills/prompts.
//! Runtime saves write **home only** so project overlays are not baked into the global file.
//!
//! # Shape (domain groups)
//!
//! ```json
//! {
//!   "preferredChatLanguage": "english",
//!   "maxRetries": 2,
//!   "defaultTimeout": "120s",
//!   "ui": { "theme": "auto", "showThinking": true, "density": "compact", ... },
//!   "models": {
//!     "defaultModel": null,
//!     "sessionTitleModel": "inherit",
//!     "compactionModel": "inherit",
//!     "treeBranchSummaries": "inherit",
//!     "defaultThinkingLevel": "high",
//!     "showConfiguredOnly": false,
//!     "scopedModels": [],
//!     "embed": { "model": "AllMiniLML6V2", "quantized": true, "gpuAcceleration": "auto" }
//!   },
//!   "promptEncoding": null,
//!   "memory": { ... },
//!   "notifications": { ... },
//!   "compaction": { "thresholdPct": 80, "keepRecentTokens": 20000, "physicalPrune": true },
//!   "session": {
//!     "retention": {
//!       "enabled": true,
//!       "gcOnOpen": true,
//!       "maxSessionsPerCwd": 40,
//!       "maxSessionAgeDays": 30,
//!       "maxEntriesPerSession": 8000,
//!       "maxStoreDbBytes": 536870912,
//!       "protectLatestPerCwd": true,
//!       "maxEntryPayloadBytes": 262144,
//!       "journalKeepTurns": 20,
//!       "maxTerminalFilesPerSession": 50
//!     }
//!   },
//!   "workers": {
//!     "enabled": true,
//!     "name": null,
//!     "purpose": "",
//!     "heartbeatSecs": 10,
//!     "leaseStaleSecs": 30,
//!     "inboxPollMs": 750,
//!     "askTimeoutMs": 600000,
//!     "maxHops": 5,
//!     "tuiShowPeers": true,
//!     "fileLeases": true
//!   }
//! }
//! ```
//!
//! **Per-session state** (active model, thinking level, agent mode) is **not** stored here —
//! it lives on the coding session / Turso session tree so concurrent Elph instances do not
//! race on `settings.json`. New sessions seed model/thinking from `models.defaultModel` /
//! `models.defaultThinkingLevel`; agent mode always starts as `build`.
//!
//! Host-only: `elph-ai` and `elph-agent` never read these paths; the binary maps fields
//! into agnostic harness options at session creation.
//!
//! Current nested shape only — no legacy key rewrite on load.

pub mod apply;
mod merge;
pub mod patterns;

use merge::deep_merge;
use std::path::{Path, PathBuf};

use crate::utils::path::AppPaths;
use anyhow::{Context, Result};
use elph_agent::write_json_file;
use elph_tui::{ThemeConfig, ThemeMode, ThemePalettes};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Value};

use super::paths::Paths;

/// Which settings file to read/write for layer-scoped operations.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SettingsScope {
    /// `CONFIG_DIR/settings.json` (default write target for runtime prefs).
    #[default]
    Home,
    /// `<project>/.elph/settings.json`.
    Project,
}

impl SettingsScope {
    pub fn label(self) -> &'static str {
        match self {
            Self::Home => "home",
            Self::Project => "project",
        }
    }
}

/// Root settings document — grouped by product domain.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    /// Preferred language for AI chat responses in the transcript.
    ///
    /// Code, comments, and documentation remain in English regardless of this setting.
    #[serde(default = "default_preferred_chat_language")]
    pub preferred_chat_language: String,
    /// Retries on 5xx / network errors for LLM HTTP transport.
    #[serde(default = "default_provider_max_retries")]
    pub max_retries: u32,
    /// Inactivity / SSE stall limit for LLM streams (e.g. `"120s"`, `"2m"`).
    #[serde(default = "default_provider_timeout")]
    pub default_timeout: String,
    /// Follow Simplified Technical English (ASD-STE100) in every response.
    /// On by default; set `false` to fall back to the plain style rules.
    #[serde(default = "default_true")]
    pub simplified_technical_english: bool,
    /// Transcript / chrome / picker presentation.
    #[serde(default)]
    pub ui: UiSettings,
    /// Model catalog preferences and **new-session** defaults (not live session state).
    #[serde(default)]
    pub models: ModelsSettings,
    /// Optional TOON prompt-encoding override for model-visible tool results.
    /// Absent / `null` falls back to `ELPH_PROMPT_ENCODING*` environment variables.
    #[serde(default)]
    pub prompt_encoding: Option<elph_agent::PromptEncodingConfig>,
    /// Local floppy memory (hooks / retrieval; embed model lives under `models.embed`).
    #[serde(default)]
    pub memory: MemorySettings,
    /// MCP client preferences (tool result cache retention).
    #[serde(default)]
    pub mcp: McpSettings,
    /// Desktop notification preferences.
    #[serde(default)]
    pub notifications: NotificationSettings,
    /// Auto-compaction preferences.
    #[serde(default)]
    pub compaction: CompactionConfig,
    /// Session storage and retention (not live model/mode state).
    #[serde(default)]
    pub session: SessionSettings,
    /// Multi-process worker coordination (leases, registry, mailbox, TUI peers).
    #[serde(default)]
    pub workers: WorkersSettings,
    /// Extra resource paths and enable/disable filters.
    #[serde(default)]
    pub resources: ResourcesSettings,
    /// Built-in tool allowlist (`null` = all builtins).
    #[serde(default)]
    pub tools: ToolsSettings,
    /// Project-trust fallback. Global layer only (project file cannot override).
    #[serde(default)]
    pub trust: TrustSettings,
    /// Shell invocation overrides for `shell_exec`.
    #[serde(default)]
    pub shell: ShellSettings,
    /// HTTP proxy for Elph-managed clients.
    #[serde(default)]
    pub network: NetworkSettings,
    /// Set at load time: project settings/resources layer was applied.
    #[serde(skip)]
    pub project_layer_loaded: bool,
}

/// Multi-worker coordination preferences (same machine / shared project store).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorkersSettings {
    /// Master switch: session lease, registry, worker tools, heartbeat.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Preferred display name among live peers (suffix allocated on collision).
    #[serde(default)]
    pub name: Option<String>,
    /// Short purpose shown in `worker_list`.
    #[serde(default)]
    pub purpose: String,
    /// Heartbeat interval for lease + registry + file claims (seconds).
    #[serde(default = "default_workers_heartbeat_secs")]
    pub heartbeat_secs: u64,
    /// Stale window before reclaim / demote (seconds). Must be > heartbeat.
    #[serde(default = "default_workers_lease_stale_secs")]
    pub lease_stale_secs: u64,
    /// Inbox poll interval for durable mailbox delivery (milliseconds).
    #[serde(default = "default_workers_inbox_poll_ms")]
    pub inbox_poll_ms: u64,
    /// Ask timeout before message marked `timeout` (milliseconds).
    #[serde(default = "default_workers_ask_timeout_ms")]
    pub ask_timeout_ms: u64,
    /// Max hops when forwarding worker messages.
    #[serde(default = "default_workers_max_hops")]
    pub max_hops: u32,
    /// Show compact peer badge in TUI when live workers ≥ 2.
    #[serde(default = "default_true")]
    pub tui_show_peers: bool,
    /// Cross-process path claims on mutate tools (shared-cwd safety).
    #[serde(default = "default_true")]
    pub file_leases: bool,
}

impl Default for WorkersSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            name: None,
            purpose: String::new(),
            heartbeat_secs: default_workers_heartbeat_secs(),
            lease_stale_secs: default_workers_lease_stale_secs(),
            inbox_poll_ms: default_workers_inbox_poll_ms(),
            ask_timeout_ms: default_workers_ask_timeout_ms(),
            max_hops: default_workers_max_hops(),
            tui_show_peers: true,
            file_leases: true,
        }
    }
}

fn default_workers_heartbeat_secs() -> u64 {
    10
}
fn default_workers_lease_stale_secs() -> u64 {
    30
}
fn default_workers_inbox_poll_ms() -> u64 {
    750
}
fn default_workers_ask_timeout_ms() -> u64 {
    600_000
}
fn default_workers_max_hops() -> u32 {
    5
}

/// Extra skill/prompt/extension paths and name filters.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ResourcesSettings {
    /// Extra skill files/directories (globs, `!` exclude, `+`/`-` exact).
    #[serde(default)]
    pub skills: Vec<String>,
    /// Extra prompt template paths.
    #[serde(default)]
    pub prompts: Vec<String>,
    /// Extra extension directory paths.
    #[serde(default)]
    pub extensions: Vec<String>,
    /// Skill names (globs) to drop after discovery.
    #[serde(default)]
    pub disabled_skills: Vec<String>,
    /// Extension names to skip (same role as former `extensions.json` `disabled`).
    #[serde(default)]
    pub disabled_extensions: Vec<String>,
    /// Register discovered skills as `/name` slash commands.
    #[serde(default = "default_true")]
    pub enable_skill_commands: bool,
}

impl Default for ResourcesSettings {
    fn default() -> Self {
        Self {
            skills: Vec::new(),
            prompts: Vec::new(),
            extensions: Vec::new(),
            disabled_skills: Vec::new(),
            disabled_extensions: Vec::new(),
            enable_skill_commands: true,
        }
    }
}

/// Built-in tool allowlist. `None` = all builtins; `Some([])` = none (meta tools stay).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ToolsSettings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<Vec<String>>,
}

/// Fallback when `trust.json` has no decision for this folder (global setting only).
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DefaultProjectTrust {
    #[default]
    Ask,
    Always,
    Never,
}

impl DefaultProjectTrust {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ask => "ask",
            Self::Always => "always",
            Self::Never => "never",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TrustSettings {
    #[serde(default)]
    pub default_project_trust: DefaultProjectTrust,
}

impl Default for TrustSettings {
    fn default() -> Self {
        Self {
            default_project_trust: DefaultProjectTrust::Ask,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ShellSettings {
    /// Custom shell binary (leading `~` expanded by the host).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Prefix prepended to every `shell_exec` command.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command_prefix: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NetworkSettings {
    /// HTTP proxy URL applied as `HTTP_PROXY` / `HTTPS_PROXY` when those env vars are unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub http_proxy: Option<String>,
}

/// Session storage preferences (retention / GC). Per-session pin is DB state, not settings.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct SessionSettings {
    #[serde(default)]
    pub retention: SessionRetentionSettings,
}

/// Automatic session GC, payload caps, and size budgets.
///
/// Numeric fields use `0` to mean “unlimited / disabled for that dimension”
/// (except booleans).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SessionRetentionSettings {
    /// Master switch for automatic session GC + size enforcement.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Run GC when opening the project store (best-effort, never mid-turn).
    #[serde(default = "default_true")]
    pub gc_on_open: bool,
    /// Keep at most N non-pinned sessions per project cwd (newest `updated_at` first).
    #[serde(default = "default_max_sessions_per_cwd")]
    pub max_sessions_per_cwd: u32,
    /// Drop non-pinned sessions older than this many days (`0` = no age limit).
    #[serde(default = "default_max_session_age_days")]
    pub max_session_age_days: u32,
    /// Soft pressure: after this many tree entries, prefer compact+prune.
    #[serde(default = "default_max_entries_per_session")]
    pub max_entries_per_session: u32,
    /// Soft budget for `.elph/store.db` file size in bytes (`0` = unlimited).
    #[serde(default = "default_max_store_db_bytes")]
    pub max_store_db_bytes: u64,
    /// Never auto-GC the most recently updated session for a cwd.
    #[serde(default = "default_true")]
    pub protect_latest_per_cwd: bool,
    /// Truncate oversized entry payloads on write (bytes).
    #[serde(default = "default_max_entry_payload_bytes")]
    pub max_entry_payload_bytes: u32,
    /// Keep harness journal custom entries covering approximately this many recent turns.
    #[serde(default = "default_journal_keep_turns")]
    pub journal_keep_turns: u32,
    /// Cap terminal output files retained per session (`0` = unlimited until session GC).
    #[serde(default = "default_max_terminal_files")]
    pub max_terminal_files_per_session: u32,
}

impl Default for SessionRetentionSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            gc_on_open: true,
            max_sessions_per_cwd: default_max_sessions_per_cwd(),
            max_session_age_days: default_max_session_age_days(),
            max_entries_per_session: default_max_entries_per_session(),
            max_store_db_bytes: default_max_store_db_bytes(),
            protect_latest_per_cwd: true,
            max_entry_payload_bytes: default_max_entry_payload_bytes(),
            journal_keep_turns: default_journal_keep_turns(),
            max_terminal_files_per_session: default_max_terminal_files(),
        }
    }
}

fn default_max_sessions_per_cwd() -> u32 {
    40
}
fn default_max_session_age_days() -> u32 {
    30
}
fn default_max_entries_per_session() -> u32 {
    8000
}
fn default_max_store_db_bytes() -> u64 {
    512 * 1024 * 1024
}
fn default_max_entry_payload_bytes() -> u32 {
    256 * 1024
}
fn default_journal_keep_turns() -> u32 {
    20
}
fn default_max_terminal_files() -> u32 {
    50
}

/// TUI presentation preferences.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UiSettings {
    /// Appearance mode: `auto` (follow terminal via `COLORFGBG`), `dark`, or `light`.
    /// Empty / null / unknown values normalize to `auto`.
    #[serde(default = "default_theme_mode", deserialize_with = "deserialize_theme_mode_string")]
    pub theme: String,
    /// Per-appearance color token overrides (`dark` / `light` maps).
    ///
    /// ```json
    /// "themes": {
    ///   "dark": { "accent": "#6699ff", "textPrimary": "rgb(212, 213, 217)" },
    ///   "light": { "codeBlockBg": "#e8eaed" }
    /// }
    /// ```
    #[serde(default, deserialize_with = "deserialize_theme_palettes")]
    pub themes: ThemePalettes,
    #[serde(default = "default_true")]
    pub show_thinking: bool,
    #[serde(default = "default_false")]
    pub auto_expand_thinking: bool,
    #[serde(default = "default_true")]
    pub sticky_scroll: bool,
    #[serde(default = "default_footer_token_display")]
    pub footer_token_display: String,
    /// When true, footer status uses mode/thinking/git accent colors; otherwise dimmed grey.
    #[serde(default = "default_true")]
    pub colored_status_footer: bool,
    /// Transcript log density: `compact` (default) packs collapsed tool-call items into a
    /// grouped log with no blank line between them; `loose` keeps the roomier spacing where
    /// every process-log row has a blank line above and below.
    /// Expanded (accessed) tool call items always keep line breaks above and below.
    /// `Thinking` and AI chat response/assistant items always keep line breaks above and below.
    #[serde(default = "default_log_density", deserialize_with = "deserialize_log_density_string")]
    pub density: String,
    #[serde(default)]
    pub file_picker: FilePickerSettings,
    /// When true, allow mode changes (keyboard shortcut and agent request)
    /// while the agent is busy streaming or running tools.
    #[serde(default = "default_true")]
    pub allow_mode_change_while_busy: bool,
    /// When true (default), show a dimmed per-turn stats card (tokens in/out, cache,
    /// provider/model) under the last assistant reply after each completed turn.
    #[serde(default = "default_true")]
    pub turn_stats: bool,
    /// Hide bootstrap spinner / startup chatter. `ELPH_QUIET` still wins when set.
    #[serde(default = "default_false")]
    pub quiet_startup: bool,
}

impl Default for UiSettings {
    fn default() -> Self {
        Self {
            theme: default_theme_mode(),
            themes: ThemePalettes::default(),
            show_thinking: true,
            auto_expand_thinking: false,
            sticky_scroll: true,
            footer_token_display: default_footer_token_display(),
            colored_status_footer: true,
            density: default_log_density(),
            file_picker: FilePickerSettings::default(),
            allow_mode_change_while_busy: true,
            turn_stats: true,
            quiet_startup: false,
        }
    }
}

impl UiSettings {
    /// Canonical theme mode string (`auto` / `dark` / `light`), never empty.
    pub fn theme_mode(&self) -> ThemeMode {
        ThemeMode::parse(&self.theme)
    }

    /// Build an elph-tui [`ThemeConfig`] from mode + dark/light token maps.
    pub fn theme_config(&self) -> ThemeConfig {
        ThemeConfig::from_mode_and_palettes(self.theme_mode(), self.themes.clone())
    }
}

/// Accept `null`, `""`, or any string; map to a canonical mode name.
fn deserialize_theme_mode_string<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<Value>::deserialize(deserializer)?;
    let mode = match value {
        None | Some(Value::Null) => ThemeMode::Auto,
        Some(Value::String(s)) if s.trim().is_empty() => ThemeMode::Auto,
        Some(Value::String(s)) => ThemeMode::parse(&s),
        // Tolerate accidental non-strings (e.g. `true`) as auto.
        Some(_) => ThemeMode::Auto,
    };
    Ok(mode.as_str().to_string())
}

/// Accept missing / null / empty object for `themes`; never fail the whole settings file.
fn deserialize_theme_palettes<'de, D>(deserializer: D) -> Result<ThemePalettes, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<Value>::deserialize(deserializer)?;
    match value {
        None | Some(Value::Null) => Ok(ThemePalettes::default()),
        Some(obj @ Value::Object(_)) => match serde_json::from_value::<ThemePalettes>(obj) {
            Ok(p) => Ok(p),
            Err(_) => Ok(ThemePalettes::default()),
        },
        Some(_) => Ok(ThemePalettes::default()),
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FilePickerSettings {
    /// When true, `@` file search includes dotfiles and dot-directories.
    #[serde(default = "default_false")]
    pub show_hidden_files: bool,
}

/// Model-catalog preferences and seeds for **new** sessions.
///
/// Live provider/model, thinking level, and agent mode are per-session — not stored here.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ModelsSettings {
    /// Optional `provider/model_id` seed for **new** sessions (e.g. `openai/gpt-5.6-luna`).
    /// Empty / omitted → no model until the user picks one (or env / CLI override).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_model: Option<String>,
    /// Model for automatic session title generation (`provider/model_id`, or `"inherit"`).
    #[serde(default = "default_inherit_model")]
    pub session_title_model: String,
    /// Model used when summarizing history during compaction (`inherit` = session model).
    #[serde(default = "default_inherit_model")]
    pub compaction_model: String,
    /// Model used for tree branch summarization (`inherit` = session model).
    #[serde(default = "default_inherit_model")]
    pub tree_branch_summaries: String,
    /// Thinking / reasoning level seed for **new** sessions (`off`…`max`).
    #[serde(default = "default_thinking_level")]
    pub default_thinking_level: String,
    /// When true (default), the model picker's **All** list and **Provider** tabs only
    /// include providers that already have credentials in `auth.json` (API key, OAuth, or env ref).
    #[serde(default = "default_true")]
    pub show_configured_only: bool,
    /// `provider/model_id` entries for Ctrl+P cycling and the model picker Scoped tab.
    /// Edit via `/scoped-models`.
    #[serde(default)]
    pub scoped_models: Vec<String>,
    /// Glob filter for the model catalog (`provider/model_id` or bare id). Empty = no filter.
    #[serde(default)]
    pub enabled: Vec<String>,
    /// Custom thinking token budgets per level (Anthropic / Google / compat).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking_budgets: Option<elph_ai::ThinkingBudgets>,
    /// Local ONNX / Hugging Face embedding model for floppy memory.
    #[serde(default)]
    pub embed: EmbedSettings,
}

impl Default for ModelsSettings {
    fn default() -> Self {
        Self {
            default_model: None,
            session_title_model: default_inherit_model(),
            compaction_model: default_inherit_model(),
            tree_branch_summaries: default_inherit_model(),
            default_thinking_level: default_thinking_level(),
            show_configured_only: false,
            scoped_models: Vec::new(),
            enabled: Vec::new(),
            thinking_budgets: None,
            embed: EmbedSettings::default(),
        }
    }
}

/// GPU acceleration mode for embeddings.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum GpuAcceleration {
    /// Always use GPU (fails if GPU unavailable or feature not enabled).
    On,
    /// Never use GPU (CPU-only).
    Off,
    /// Auto-detect and use GPU if available (default).
    #[default]
    Auto,
}

impl std::fmt::Display for GpuAcceleration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GpuAcceleration::On => write!(f, "on"),
            GpuAcceleration::Off => write!(f, "off"),
            GpuAcceleration::Auto => write!(f, "auto"),
        }
    }
}

/// Local embedding model for vector search (memory).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EmbedSettings {
    /// Embedding model catalog name or Hugging Face repo id (see `floppy::resolve_embedding_model`).
    #[serde(default = "default_embed_model", deserialize_with = "deserialize_embed_model")]
    pub model: EmbedModel,
    /// Prefer quantized model weights when a `*Q` variant exists (default: true).
    #[serde(default = "default_embed_quantized")]
    pub quantized: bool,
    /// GPU acceleration mode: on (always), off (never), auto (detect, default).
    #[serde(default = "default_gpu_acceleration")]
    pub gpu_acceleration: GpuAcceleration,
}

impl Default for EmbedSettings {
    fn default() -> Self {
        Self {
            model: default_embed_model(),
            quantized: default_embed_quantized(),
            gpu_acceleration: default_gpu_acceleration(),
        }
    }
}

/// Supported embedding models for local vector search.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum EmbedModel {
    /// Fast, small BERT model (sentence-transformers/all-MiniLM-L6-v2). 384 dims, ~20MB.
    AllMiniLML6V2,
    /// Jina v2 base English model (jinaai/jina-embeddings-v2-base-en). 768 dims, better quality.
    JinaEmbeddingsV2BaseEn,
    /// Jina v2 small English model (jinaai/jina-embeddings-v2-small-en). 512 dims, faster.
    JinaEmbeddingsV2SmallEn,
    /// Nomic embed text v1.5 (nomic-ai/nomic-embed-text-v1.5). 768 dims, 8192 context.
    NomicEmbedTextV15,
    /// Qwen3 embedding 0.6B (Qwen/Qwen3-Embedding-0.6B). Modern, good balance.
    Qwen3Embedding06B,
    /// Gemma3 embedding 300M (google/embeddinggemma-300m). Google's lightweight model.
    GemmaEmbedding300M,
    /// Custom Hugging Face repo ID (e.g., "BAAI/bge-small-en-v1.5").
    Custom(String),
}

impl EmbedModel {
    /// Convert to the string format expected by embed_anything.
    pub fn to_model_id(&self) -> String {
        match self {
            Self::AllMiniLML6V2 => "sentence-transformers/all-MiniLM-L6-v2".to_string(),
            Self::JinaEmbeddingsV2BaseEn => "jinaai/jina-embeddings-v2-base-en".to_string(),
            Self::JinaEmbeddingsV2SmallEn => "jinaai/jina-embeddings-v2-small-en".to_string(),
            Self::NomicEmbedTextV15 => "nomic-ai/nomic-embed-text-v1.5".to_string(),
            Self::Qwen3Embedding06B => "Qwen/Qwen3-Embedding-0.6B".to_string(),
            Self::GemmaEmbedding300M => "google/embeddinggemma-300m".to_string(),
            Self::Custom(id) => id.clone(),
        }
    }

    /// Parse from string (for backwards compatibility with old settings).
    pub fn from_string(s: &str) -> Self {
        match s {
            "allMiniLML6V2" | "AllMiniLML6V2" | "sentence-transformers/all-minilm-l6-v2" => Self::AllMiniLML6V2,
            "jinaEmbeddingsV2BaseEn" | "JinaEmbeddingsV2BaseEn" | "jinaai/jina-embeddings-v2-base-en" => {
                Self::JinaEmbeddingsV2BaseEn
            }
            "jinaEmbeddingsV2SmallEn" | "JinaEmbeddingsV2SmallEn" | "jinaai/jina-embeddings-v2-small-en" => {
                Self::JinaEmbeddingsV2SmallEn
            }
            "nomicEmbedTextV15" | "NomicEmbedTextV15" | "nomic-ai/nomic-embed-text-v1.5" => Self::NomicEmbedTextV15,
            "qwen3Embedding06B" | "Qwen3Embedding06B" | "qwen/qwen3-embedding-0.6b" => Self::Qwen3Embedding06B,
            "gemmaEmbedding300M" | "GemmaEmbedding300M" | "google/embeddinggemma-300m" => Self::GemmaEmbedding300M,
            _ => Self::Custom(s.to_string()),
        }
    }
}

/// Custom deserializer for EmbedModel to support both enum and string (backwards compatibility).
fn deserialize_embed_model<'de, D>(deserializer: D) -> Result<EmbedModel, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<Value>::deserialize(deserializer)?;
    match value {
        None | Some(Value::Null) => Ok(default_embed_model()),
        Some(Value::String(s)) => Ok(EmbedModel::from_string(&s)),
        Some(other) => {
            // Try to serialize and deserialize as string
            let json = serde_json::to_string(&other).map_err(serde::de::Error::custom)?;
            Ok(EmbedModel::from_string(&json))
        }
    }
}

impl ModelsSettings {
    /// Split `defaultModel` into `(provider, model_id)` when well-formed.
    pub fn default_provider_and_model(&self) -> Option<(String, String)> {
        let raw = self.default_model.as_deref()?.trim();
        if raw.is_empty() || raw.eq_ignore_ascii_case("inherit") {
            return None;
        }
        let (provider, model) = raw.split_once('/')?;
        let provider = provider.trim();
        let model = model.trim();
        if provider.is_empty() || model.is_empty() {
            return None;
        }
        Some((provider.to_string(), model.to_string()))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MemorySettings {
    /// Master switch for automatic memory hooks and bootstrap injection.
    /// Agent tools can still open the store when disabled.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Per-turn multi-source recall injection.
    #[serde(default = "default_true")]
    pub auto_recall: bool,
    /// Auto-journal successful file mutations as work/change memories.
    #[serde(default = "default_true")]
    pub auto_capture_work: bool,
    /// Auto-capture exploration into discovery / project-map memories.
    #[serde(default = "default_true")]
    pub auto_capture_exploration: bool,
    /// Vector top-k for task retrieval (default: 5).
    #[serde(default = "default_memory_top_k")]
    pub top_k: u32,
    /// Max characters for all injected memory XML blocks (default: 3000).
    #[serde(default = "default_memory_context_budget")]
    pub context_budget_chars: u32,
    /// Minimum user prompt length to trigger auto-recall (default: 15).
    #[serde(default = "default_memory_min_query_length")]
    pub min_query_length: u32,
}

impl Default for MemorySettings {
    fn default() -> Self {
        Self {
            enabled: true,
            auto_recall: true,
            auto_capture_work: true,
            auto_capture_exploration: true,
            top_k: default_memory_top_k(),
            context_budget_chars: default_memory_context_budget(),
            min_query_length: default_memory_min_query_length(),
        }
    }
}

/// MCP client preferences (tool result cache retention).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct McpSettings {
    /// Tool result cache TTL in seconds (default 60). `0` disables caching.
    #[serde(default = "default_mcp_cache_ttl_secs")]
    pub cache_ttl_secs: u64,
    /// Max cache entries before eviction (default 2048).
    #[serde(default = "default_mcp_cache_max_entries")]
    pub cache_max_entries: usize,
}

impl Default for McpSettings {
    fn default() -> Self {
        Self {
            cache_ttl_secs: default_mcp_cache_ttl_secs(),
            cache_max_entries: default_mcp_cache_max_entries(),
        }
    }
}

fn default_mcp_cache_ttl_secs() -> u64 {
    60
}

fn default_mcp_cache_max_entries() -> usize {
    2048
}

/// Desktop notification preferences.
///
/// Controls which events trigger native OS notifications
/// (macOS Notification Center, Linux D-Bus, Windows Toast).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NotificationSettings {
    /// Master switch — disable all desktop notifications.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Notify when the agent finishes a turn.
    #[serde(default = "default_true")]
    pub on_turn_complete: bool,
    /// Notify when the agent requests tool permission.
    #[serde(default = "default_true")]
    pub on_tool_permission: bool,
    /// Notify when the agent asks a question.
    #[serde(default = "default_true")]
    pub on_user_question: bool,
    /// Notify on errors (agent / MCP / bootstrap failure).
    #[serde(default = "default_true")]
    pub on_error: bool,
    /// Notify when a running turn is canceled.
    #[serde(default = "default_false")]
    pub on_turn_cancel: bool,
    /// Notify when bootstrap / startup completes.
    #[serde(default = "default_true")]
    pub on_startup_ready: bool,
    /// Minimum turn duration (seconds) before sending a turn-complete notification.
    /// Prevents noise from quick turns.
    #[serde(default = "default_min_turn_duration_secs")]
    pub min_turn_duration_secs: f64,
    /// Application name shown in the notification banner.
    #[serde(default = "default_notification_app_name")]
    pub app_name: String,
}

impl Default for NotificationSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            on_turn_complete: true,
            on_tool_permission: true,
            on_user_question: true,
            on_error: true,
            on_turn_cancel: false,
            on_startup_ready: true,
            min_turn_duration_secs: 5.0,
            app_name: "Elph".to_string(),
        }
    }
}

/// Auto-compaction preferences (threshold / keep-recent only).
///
/// Automatic compaction is always on after turns when usage exceeds the threshold.
/// Manual `/compact` is always available. There is no settings kill-switch.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CompactionConfig {
    /// Context-window usage percentage that triggers auto-compaction (1–100).
    /// Compact when context tokens exceed `context_window * threshold_pct / 100`.
    #[serde(default = "default_compaction_threshold_pct")]
    pub threshold_pct: u8,
    /// Number of recent tokens to keep after compaction.
    #[serde(default = "default_compaction_keep_recent")]
    pub keep_recent_tokens: u64,
    /// When true (default), physically DELETE pre-boundary `session_entries` after compact.
    #[serde(default = "default_true")]
    pub physical_prune: bool,
    /// Tokens reserved for the model response during compaction.
    #[serde(default = "default_compaction_reserve_tokens")]
    pub reserve_tokens: u64,
}

impl Default for CompactionConfig {
    fn default() -> Self {
        Self {
            threshold_pct: default_compaction_threshold_pct(),
            keep_recent_tokens: default_compaction_keep_recent(),
            physical_prune: true,
            reserve_tokens: default_compaction_reserve_tokens(),
        }
    }
}

impl CompactionConfig {
    /// Convert to elph-agent's `CompactionSettings`.
    ///
    /// Auto-compaction is always enabled at the host; only threshold/keep-recent are user-tunable.
    pub fn to_agent_settings(&self) -> elph_agent::CompactionSettings {
        elph_agent::CompactionSettings {
            enabled: true,
            reserve_tokens: self.reserve_tokens,
            threshold_pct: Some(self.threshold_pct.clamp(1, 100)),
            keep_recent_tokens: self.keep_recent_tokens,
            physical_prune: self.physical_prune,
        }
    }
}

impl Settings {
    /// Built-in defaults written on first bootstrap (`Settings::ensure`).
    ///
    /// No model is pre-selected: `models.defaultModel` and `models.scopedModels`
    /// stay empty until the user configures them. Live session model/mode/thinking
    /// are never written here.
    pub fn defaults() -> Self {
        Self {
            preferred_chat_language: default_preferred_chat_language(),
            max_retries: default_provider_max_retries(),
            default_timeout: default_provider_timeout(),
            simplified_technical_english: default_true(),
            ui: UiSettings::default(),
            models: ModelsSettings::default(),
            prompt_encoding: None,
            memory: MemorySettings::default(),
            mcp: McpSettings::default(),
            notifications: NotificationSettings::default(),
            compaction: CompactionConfig::default(),
            session: SessionSettings::default(),
            workers: WorkersSettings::default(),
            resources: ResourcesSettings::default(),
            tools: ToolsSettings::default(),
            trust: TrustSettings::default(),
            shell: ShellSettings::default(),
            network: NetworkSettings::default(),
            project_layer_loaded: false,
        }
    }

    /// Parse top-level `defaultTimeout` into milliseconds for stream options.
    pub fn provider_timeout_ms(&self) -> Option<u64> {
        parse_duration_ms(&self.default_timeout)
    }

    /// Path for a single layer.
    pub fn path_for(paths: &Paths, scope: SettingsScope) -> PathBuf {
        match scope {
            SettingsScope::Home => paths.settings_path(),
            SettingsScope::Project => paths.project_settings_path(),
        }
    }

    /// Create home `settings.json` with defaults when missing.
    pub fn ensure(paths: &Paths) -> Result<()> {
        let path = paths.settings_path();
        if path.exists() {
            return Ok(());
        }

        write_json_file(&path, &Self::defaults())?;
        Ok(())
    }

    /// Load one layer (missing file → empty object, then serde defaults).
    pub fn load_layer(paths: &Paths, scope: SettingsScope) -> Result<Self> {
        let path = Self::path_for(paths, scope);
        let value = read_settings_value(&path)?;
        serde_json::from_value(value).with_context(|| format!("parse {}", path.display()))
    }

    /// Whether project-local **WASM extensions** may load.
    ///
    /// Settings JSON, skills, and prompts are not gated. `trust.*` is read from home only.
    /// `ask` without a saved `trust.json` decision is treated as `never` (no prompt this pass).
    pub fn project_extensions_allowed(paths: &Paths, home: &Self) -> bool {
        if crate::platform::scaffold::TrustStore::is_trusted(paths, paths.project_dir()).unwrap_or(false) {
            return true;
        }
        matches!(home.trust.default_project_trust, DefaultProjectTrust::Always)
    }

    /// Load merged settings: serde defaults ← home ← project (project always wins).
    /// `trust.*` always comes from the home layer.
    pub fn load(paths: &Paths) -> Result<Self> {
        Self::ensure(paths)?;
        let home_value = read_settings_value(&paths.settings_path())?;
        let home: Self = serde_json::from_value(home_value.clone()).context("parse home settings")?;
        let project = read_settings_value(&paths.project_settings_path())?;
        let mut merged = home_value;
        deep_merge(&mut merged, &project);
        let mut settings: Self = serde_json::from_value(merged).context("parse merged settings")?;
        settings.trust = home.trust;
        settings.project_layer_loaded = paths.project_settings_path().is_file();
        Ok(settings)
    }

    /// Load home layer only (for mutations that must not bake project overrides).
    pub fn load_home(paths: &Paths) -> Result<Self> {
        Self::ensure(paths)?;
        Self::load_layer(paths, SettingsScope::Home)
    }

    /// Persist settings to the home layer only.
    pub fn save(paths: &Paths, settings: &Self) -> Result<()> {
        Self::save_layer(paths, SettingsScope::Home, settings)
    }

    /// Persist a specific layer. Creates parent dirs for project scope.
    pub fn save_layer(paths: &Paths, scope: SettingsScope, settings: &Self) -> Result<()> {
        let path = Self::path_for(paths, scope);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
        }
        write_json_file(&path, settings).with_context(|| format!("write {}", path.display()))
    }
}

fn read_settings_value(path: &Path) -> Result<Value> {
    if !path.exists() {
        return Ok(Value::Object(Map::new()));
    }
    let raw = std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    if raw.trim().is_empty() {
        return Ok(Value::Object(Map::new()));
    }
    serde_json::from_str(&raw).with_context(|| format!("parse {}", path.display()))
}

/// Parse compact duration strings used in settings (`120s`, `2m`, `24h`, plain ms digits).
fn parse_duration_ms(input: &str) -> Option<u64> {
    let s = input.trim();
    if s.is_empty() {
        return None;
    }
    if let Ok(ms) = s.parse::<u64>() {
        return Some(ms);
    }
    let (num, unit) = s.split_at(s.len().saturating_sub(1));
    let n: u64 = num.trim().parse().ok()?;
    match unit {
        "s" | "S" => Some(n.saturating_mul(1_000)),
        "m" | "M" => Some(n.saturating_mul(60_000)),
        "h" | "H" => Some(n.saturating_mul(3_600_000)),
        _ => None,
    }
}

fn default_embed_model() -> EmbedModel {
    EmbedModel::AllMiniLML6V2
}

fn default_embed_quantized() -> bool {
    true
}

fn default_gpu_acceleration() -> GpuAcceleration {
    GpuAcceleration::Auto
}

fn default_memory_top_k() -> u32 {
    8
}

fn default_memory_context_budget() -> u32 {
    4000
}

fn default_memory_min_query_length() -> u32 {
    8
}

fn default_thinking_level() -> String {
    "high".to_string()
}

fn default_inherit_model() -> String {
    "inherit".to_string()
}

fn default_preferred_chat_language() -> String {
    "english".to_string()
}

fn default_footer_token_display() -> String {
    "both".to_string()
}

fn default_theme_mode() -> String {
    "auto".to_string()
}

fn default_provider_max_retries() -> u32 {
    2
}

fn default_provider_timeout() -> String {
    "120s".to_string()
}

fn default_true() -> bool {
    true
}

fn default_false() -> bool {
    false
}

/// Canonical transcript log density (`compact` default).
fn default_log_density() -> String {
    "compact".to_string()
}

/// Accept missing / null / empty / unknown values and canonicalize to `compact` or `loose`.
fn deserialize_log_density_string<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<Value>::deserialize(deserializer)?;
    let density = match value {
        None | Some(Value::Null) => default_log_density(),
        Some(Value::String(s)) => {
            let trimmed = s.trim().to_ascii_lowercase();
            match trimmed.as_str() {
                "compact" | "loose" => trimmed,
                // Tolerate the former boolean key (true was compact).
                "true" => "compact".to_string(),
                "false" => "loose".to_string(),
                _ => default_log_density(),
            }
        }
        // Tolerate accidental non-strings (e.g. `true` / `false`) as compact / loose.
        Some(Value::Bool(true)) => "compact".to_string(),
        Some(Value::Bool(false)) => "loose".to_string(),
        Some(_) => default_log_density(),
    };
    Ok(density)
}

fn default_min_turn_duration_secs() -> f64 {
    5.0
}

fn default_notification_app_name() -> String {
    "Elph".to_string()
}

fn default_compaction_threshold_pct() -> u8 {
    80
}

fn default_compaction_keep_recent() -> u64 {
    20_000
}

fn default_compaction_reserve_tokens() -> u64 {
    16_384
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_paths(tmp: &tempfile::TempDir) -> Paths {
        Paths::from_dirs(tmp.path().join("config"), tmp.path().join("data"), tmp.path().join("repo"))
    }

    #[test]
    fn default_settings_round_trip() {
        let settings = Settings::defaults();
        let json = serde_json::to_string_pretty(&settings).expect("serialize");
        let decoded: Settings = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(settings, decoded);
        assert_eq!(decoded.models.embed.model, EmbedModel::AllMiniLML6V2);
        assert!(decoded.models.embed.quantized);
        assert_eq!(decoded.models.embed.gpu_acceleration, GpuAcceleration::Auto);
        assert!(decoded.models.default_model.is_none());
        assert_eq!(decoded.models.session_title_model, "inherit");
        assert_eq!(decoded.models.compaction_model, "inherit");
        assert_eq!(decoded.models.tree_branch_summaries, "inherit");
        assert_eq!(decoded.models.default_thinking_level, "high");
        assert_eq!(decoded.preferred_chat_language, "english");
        assert_eq!(decoded.max_retries, 2);
        assert_eq!(decoded.default_timeout, "120s");
        assert!(decoded.simplified_technical_english);
        assert!(decoded.ui.show_thinking);
        assert!(decoded.ui.turn_stats);
        assert_eq!(decoded.ui.theme, "auto");
        assert!(decoded.ui.themes.dark.is_empty());
        assert!(decoded.models.scoped_models.is_empty());
    }

    #[test]
    fn theme_overrides_round_trip() {
        let json = r##"{
            "ui": {
                "theme": "dark",
                "themes": {
                    "dark": { "accent": "#ff0000", "textPrimary": "rgb(200, 200, 200)" },
                    "light": { "accent": "#0000ff" }
                }
            }
        }"##;
        let decoded: Settings = serde_json::from_str(json).expect("decode");
        assert_eq!(decoded.ui.theme, "dark");
        assert_eq!(decoded.ui.themes.dark.accent.as_deref(), Some("#ff0000"));
        let cfg = decoded.ui.theme_config();
        let theme = cfg.resolve();
        assert_eq!(theme.accent, elph_tui::rgb(255, 0, 0));
    }

    #[test]
    fn empty_or_null_theme_fields_normalize_to_auto() {
        for json in [
            r#"{"ui":{"theme":""}}"#,
            r#"{"ui":{"theme":"   "}}"#,
            r#"{"ui":{"theme":null}}"#,
            r#"{"ui":{"themes":null}}"#,
            r#"{"ui":{}}"#,
        ] {
            let decoded: Settings = serde_json::from_str(json).expect(json);
            assert_eq!(decoded.ui.theme, "auto", "json={json}");
            assert_eq!(decoded.ui.theme_mode(), ThemeMode::Auto);
            let _ = decoded.ui.theme_config().resolve();
        }
    }

    #[test]
    fn ensure_bootstrap_has_no_preselected_model() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let paths = test_paths(&tmp);
        Settings::ensure(&paths).expect("ensure");
        let loaded = Settings::load_home(&paths).expect("load");
        assert!(loaded.models.default_model.is_none());
        assert!(loaded.models.scoped_models.is_empty());

        let raw = std::fs::read_to_string(paths.settings_path()).expect("read");
        let value: Value = serde_json::from_str(&raw).expect("parse");
        // Storage/retention group is present; no live model state under session.
        assert!(value.get("session").is_some());
        assert!(value["session"].get("retention").is_some());
        assert!(value["session"].get("agentMode").is_none());
        assert!(value["models"].get("defaultModel").is_none());
        assert_eq!(value["models"]["scopedModels"], serde_json::json!([]));
    }

    #[test]
    fn nested_shape_serializes_domain_groups() {
        let json = serde_json::to_value(Settings::defaults()).expect("ser");
        let obj = json.as_object().expect("object");
        assert!(obj.contains_key("ui"));
        assert!(obj.contains_key("preferredChatLanguage"));
        assert!(obj.contains_key("session"));
        assert!(obj["session"].get("retention").is_some());
        assert_eq!(obj["session"]["retention"]["maxSessionsPerCwd"], 40);
        assert!(obj.contains_key("models"));
        assert!(!obj.contains_key("provider"));
        assert!(obj.contains_key("maxRetries"));
        assert!(obj.contains_key("defaultTimeout"));
        assert!(obj.contains_key("memory"));
        assert!(!obj.contains_key("showThinking"));
        assert!(!obj.contains_key("scopedModelItems"));
        assert_eq!(json["ui"]["footerTokenDisplay"], "both");
        assert_eq!(json["ui"]["turnStats"], true);
        assert!(json["models"]["scopedModels"].as_array().expect("arr").is_empty());
        assert_eq!(json["compaction"]["physicalPrune"], true);
    }

    #[test]
    fn density_setting_normalizes_unknown_values() {
        let decode = |ui: &str| -> String {
            let value: Value = serde_json::from_str(&format!(r#"{{ "ui": {ui} }}"#)).expect("parse");
            let decoded: Settings = serde_json::from_value(value).expect("decode");
            decoded.ui.density
        };
        assert_eq!(decode(r#"{ "density": "compact" }"#), "compact");
        assert_eq!(decode(r#"{ "density": "loose" }"#), "loose");
        assert_eq!(decode(r#"{ "density": "LOOSE" }"#), "loose");
        assert_eq!(decode(r#"{ "density": "wide" }"#), "compact");
        assert_eq!(decode(r#"{ "density": true }"#), "compact");
        assert_eq!(decode(r#"{ "density": false }"#), "loose");
        assert_eq!(decode(r#"{ }"#), "compact");
    }

    #[test]
    fn file_picker_settings_default_hidden_off() {
        let settings = Settings::defaults();
        assert!(!settings.ui.file_picker.show_hidden_files);
    }

    #[test]
    fn load_merges_missing_memory_section() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let paths = test_paths(&tmp);
        Settings::ensure(&paths).expect("ensure");
        let loaded = Settings::load(&paths).expect("load");
        assert_eq!(loaded.models.embed.model, EmbedModel::AllMiniLML6V2);
        // New knobs default when section/fields are missing.
        assert!(loaded.memory.enabled);
        assert!(loaded.memory.auto_recall);
        assert!(loaded.memory.auto_capture_work);
        assert!(loaded.memory.auto_capture_exploration);
        assert_eq!(loaded.memory.top_k, 8);
        assert_eq!(loaded.memory.context_budget_chars, 4000);
        assert_eq!(loaded.memory.min_query_length, 8);
        assert_eq!(loaded.models.embed.gpu_acceleration, GpuAcceleration::Auto);
    }

    #[test]
    fn memory_settings_partial_json_defaults() {
        let raw = r#"{"enabled":true,"topK":3}"#;
        let decoded: MemorySettings = serde_json::from_str(raw).expect("parse");
        assert!(decoded.enabled);
        assert!(decoded.auto_recall);
        assert_eq!(decoded.top_k, 3);
    }

    #[test]
    fn ensure_writes_only_when_missing() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let paths = test_paths(&tmp);

        Settings::ensure(&paths).expect("first ensure");
        assert!(paths.settings_path().exists());

        let before = std::fs::read_to_string(paths.settings_path()).expect("read settings");
        Settings::ensure(&paths).expect("second ensure");
        let after = std::fs::read_to_string(paths.settings_path()).expect("read settings");
        assert_eq!(before, after);
        assert!(before.contains("\"ui\""));
        assert!(before.contains("\"models\""));
    }

    #[test]
    fn project_overrides_home_fields() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let paths = test_paths(&tmp);

        Settings::ensure(&paths).expect("ensure home");
        let mut home = Settings::load_home(&paths).expect("load home");
        home.ui.show_thinking = true;
        home.ui.sticky_scroll = true;
        home.models.default_model = Some("openai/gpt-5.6-luna".into());
        home.models.default_thinking_level = "high".into();
        Settings::save(&paths, &home).expect("save home");

        let project = serde_json::json!({
            "ui": { "showThinking": false },
            "models": { "defaultThinkingLevel": "low" }
        });
        std::fs::create_dir_all(paths.project_elph_dir()).expect("project dir");
        std::fs::write(
            paths.project_settings_path(),
            serde_json::to_string_pretty(&project).expect("ser"),
        )
        .expect("write project");

        let merged = Settings::load(&paths).expect("load merged");
        assert!(!merged.ui.show_thinking);
        assert!(merged.ui.sticky_scroll);
        assert_eq!(merged.models.default_thinking_level, "low");
        assert_eq!(merged.models.default_model.as_deref(), Some("openai/gpt-5.6-luna"));
    }

    #[test]
    fn project_can_override_default_model() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let paths = test_paths(&tmp);
        Settings::ensure(&paths).expect("ensure");

        let mut home = Settings::load_home(&paths).expect("home");
        home.models.default_model = Some("openai/gpt-5.6-luna".into());
        Settings::save(&paths, &home).expect("save home");

        let project = serde_json::json!({
            "models": {
                "defaultModel": "anthropic/claude-sonnet-4"
            }
        });
        std::fs::create_dir_all(paths.project_elph_dir()).expect("project dir");
        std::fs::write(
            paths.project_settings_path(),
            serde_json::to_string_pretty(&project).expect("ser"),
        )
        .expect("write project");

        let merged = Settings::load(&paths).expect("merged");
        assert_eq!(merged.models.default_model.as_deref(), Some("anthropic/claude-sonnet-4"));
        assert_eq!(
            merged.models.default_provider_and_model(),
            Some(("anthropic".into(), "claude-sonnet-4".into()))
        );
    }

    #[test]
    fn save_writes_home_only_not_project() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let paths = test_paths(&tmp);
        Settings::ensure(&paths).expect("ensure");

        std::fs::create_dir_all(paths.project_elph_dir()).expect("project dir");
        std::fs::write(paths.project_settings_path(), r#"{"ui":{"showThinking":false}}"#).expect("project");

        let mut home = Settings::load_home(&paths).expect("home");
        home.ui.show_thinking = true;
        Settings::save(&paths, &home).expect("save");

        let home_raw: Value =
            serde_json::from_str(&std::fs::read_to_string(paths.settings_path()).expect("read")).expect("parse");
        assert_eq!(home_raw["ui"]["showThinking"], true);

        let project_raw = std::fs::read_to_string(paths.project_settings_path()).expect("read project");
        assert!(project_raw.contains("false"));
    }

    #[test]
    fn prompt_encoding_group_parses_from_settings_file() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let paths = test_paths(&tmp);
        Settings::ensure(&paths).expect("ensure");

        let home = serde_json::json!({
            "promptEncoding": {
                "mode": "auto",
                "minBytes": 4096,
                "delimiter": "pipe",
                "targets": { "structuredDetails": false }
            }
        });
        std::fs::write(paths.settings_path(), serde_json::to_string_pretty(&home).expect("ser")).expect("write home");

        let loaded = Settings::load(&paths).expect("load");
        let config = loaded.prompt_encoding.expect("promptEncoding present");
        assert_eq!(config.mode, elph_agent::PromptEncodingMode::Auto);
        assert_eq!(config.min_bytes, 4096);
        assert_eq!(config.delimiter, elph_agent::PromptEncodingDelimiter::Pipe);
        // Field defaults fill the rest.
        assert_eq!(config.min_savings_ratio, 1.0);
        assert!(config.targets.tool_result_text);
        assert!(!config.targets.structured_details);
    }

    #[test]
    fn prompt_encoding_absent_or_null_stays_none() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let paths = test_paths(&tmp);
        Settings::ensure(&paths).expect("ensure");
        assert!(Settings::load(&paths).expect("load").prompt_encoding.is_none());

        let home = serde_json::json!({ "promptEncoding": null });
        std::fs::write(paths.settings_path(), serde_json::to_string_pretty(&home).expect("ser")).expect("write home");
        assert!(Settings::load(&paths).expect("load").prompt_encoding.is_none());
    }

    #[test]
    fn load_home_ignores_project_overlay() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let paths = test_paths(&tmp);
        Settings::ensure(&paths).expect("ensure");

        let mut home = Settings::load_home(&paths).expect("home");
        home.ui.show_thinking = true;
        Settings::save(&paths, &home).expect("save");

        std::fs::create_dir_all(paths.project_elph_dir()).expect("project dir");
        std::fs::write(paths.project_settings_path(), r#"{"ui":{"showThinking":false}}"#).expect("project");

        let home_only = Settings::load_home(&paths).expect("load_home");
        assert!(home_only.ui.show_thinking);
        let merged = Settings::load(&paths).expect("load");
        assert!(!merged.ui.show_thinking);
    }

    #[test]
    fn untrusted_project_settings_still_merge() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let paths = test_paths(&tmp);
        Settings::ensure(&paths).expect("ensure");
        let mut home = Settings::load_home(&paths).expect("home");
        home.trust.default_project_trust = DefaultProjectTrust::Ask;
        home.ui.show_thinking = true;
        home.preferred_chat_language = "english".into();
        Settings::save(&paths, &home).expect("save");
        std::fs::create_dir_all(paths.project_elph_dir()).expect("project dir");
        std::fs::write(
            paths.project_settings_path(),
            r#"{"preferredChatLanguage":"Indonesian","ui":{"showThinking":false}}"#,
        )
        .expect("project");
        let loaded = Settings::load(&paths).expect("load");
        assert!(!loaded.ui.show_thinking);
        assert_eq!(loaded.preferred_chat_language, "Indonesian");
        assert!(!Settings::project_extensions_allowed(&paths, &loaded));
    }

    #[test]
    fn parse_duration_ms_units() {
        assert_eq!(parse_duration_ms("120s"), Some(120_000));
        assert_eq!(parse_duration_ms("2m"), Some(120_000));
        assert_eq!(parse_duration_ms("1h"), Some(3_600_000));
        assert_eq!(parse_duration_ms("500"), Some(500));
        assert_eq!(parse_duration_ms(""), None);
        assert_eq!(parse_duration_ms("nope"), None);
    }

    #[test]
    fn save_overwrites_existing_home_file() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let paths = test_paths(&tmp);
        Settings::ensure(&paths).expect("ensure");
        let mut s = Settings::load_home(&paths).expect("load");
        s.ui.show_thinking = false;
        Settings::save(&paths, &s).expect("first save");
        s.ui.show_thinking = true;
        Settings::save(&paths, &s).expect("second save");
        let loaded = Settings::load_home(&paths).expect("reload");
        assert!(loaded.ui.show_thinking);
    }
}
