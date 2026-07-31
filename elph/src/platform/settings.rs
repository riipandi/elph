//! Elph user settings: home + project layers.
//!
//! # Layers
//!
//! | Layer    | Path                              | Role                                      |
//! |----------|-----------------------------------|-------------------------------------------|
//! | Defaults | (in code)                         | Serde field defaults for missing keys     |
//! | Home     | `~/.elph/settings.json`           | Global prefs; default write target        |
//! | Project  | `<project>/.elph/settings.json`   | Per-repo overrides (optional)             |
//!
//! Runtime load merges **home ← project** (project wins per field / nested object key).
//! Runtime saves write **home only** so project overlays are not baked into the global file.
//!
//! # Shape (domain groups)
//!
//! ```json
//! {
//!   "ui": { ... },
//!   "session": { "providerId", "modelId", "agentMode", "thinkingLevel", "titleModel", "preferredChatLanguage" },
//!   "models": { "scoped": [...], "showConfiguredOnly": true },
//!   "provider": { ... },
//!   "memory": { ... },
//!   "notifications": { ... },
//!   "compaction": { ... }
//! }
//! ```
//!
//! Host-only: `elph-ai` and `elph-agent` never read these paths; the binary maps fields
//! into agnostic harness options at session creation.
//!
//! Flat legacy keys (e.g. top-level `showThinking`, `scopedModelItems`) are migrated on
//! load and rewritten in the nested shape on the next save.

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
    /// `~/.elph/settings.json` (default write target for runtime prefs).
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
    /// Transcript / chrome / picker presentation.
    #[serde(default)]
    pub ui: UiSettings,
    /// Last / preferred interactive session state.
    #[serde(default)]
    pub session: SessionSettings,
    /// Model catalog preferences (scoped cycle list, etc.).
    #[serde(default)]
    pub models: ModelsSettings,
    /// LLM HTTP transport defaults (mapped into harness stream options).
    #[serde(default)]
    pub provider: ProviderHttpSettings,
    /// Local embedding / floppy memory.
    #[serde(default)]
    pub memory: MemorySettings,
    /// Desktop notification preferences.
    #[serde(default)]
    pub notifications: NotificationSettings,
    /// Auto-compaction preferences.
    #[serde(default)]
    pub compaction: CompactionConfig,
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
    #[serde(default)]
    pub file_picker: FilePickerSettings,
    /// When true, allow mode changes (keyboard shortcut and agent request)
    /// while the agent is busy streaming or running tools.
    #[serde(default = "default_true")]
    pub allow_mode_change_while_busy: bool,
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
            file_picker: FilePickerSettings::default(),
            allow_mode_change_while_busy: true,
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

/// Restored session defaults (provider/model/mode/thinking).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SessionSettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
    #[serde(default = "default_agent_mode")]
    pub agent_mode: String,
    #[serde(default = "default_thinking_level")]
    pub thinking_level: String,
    /// Model for automatic session title generation (`provider/model_id`, or `"inherit"`).
    ///
    /// When `"inherit"` (default), the live session model is used for the background title call.
    #[serde(default = "default_title_model")]
    pub title_model: String,
    /// Preferred language for AI chat responses in the transcript.
    ///
    /// Code, comments, and documentation remain in English regardless of this setting.
    #[serde(default = "default_preferred_chat_language")]
    pub preferred_chat_language: String,
}

/// Model-catalog preferences.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ModelsSettings {
    /// `provider/model_id` entries for Ctrl+P cycling and the model picker Scoped tab.
    /// Edit via `/scoped-models`.
    #[serde(default)]
    pub scoped: Vec<String>,
    /// When true (default), the model picker's **All** list and **Provider** tabs only
    /// include providers that already have credentials in `auth.json` (API key, OAuth, or env ref).
    #[serde(default = "default_true")]
    pub show_configured_only: bool,
}

impl Default for ModelsSettings {
    fn default() -> Self {
        Self {
            scoped: Vec::new(),
            show_configured_only: true,
        }
    }
}

/// Provider HTTP transport preferences (mapped into harness stream options by the host).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderHttpSettings {
    /// Retries on 5xx / network errors.
    #[serde(default = "default_provider_max_retries")]
    pub max_retries: u32,
    /// Inactivity / SSE stall limit (e.g. `"120s"`, `"2m"`).
    #[serde(default = "default_provider_timeout")]
    pub default_timeout: String,
}

impl Default for ProviderHttpSettings {
    fn default() -> Self {
        Self {
            max_retries: default_provider_max_retries(),
            default_timeout: default_provider_timeout(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MemorySettings {
    /// Embedding model catalog name or Hugging Face repo id (see `floppy::resolve_embedding_model`).
    #[serde(default = "default_embed_model")]
    pub embed_model: String,
    /// Prefer quantized model weights when a `*Q` variant exists (default: true).
    #[serde(default = "default_embed_quantized")]
    pub embed_quantized: bool,
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
            embed_model: default_embed_model(),
            embed_quantized: default_embed_quantized(),
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

/// Auto-compaction preferences.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CompactionConfig {
    /// Master switch — enable automatic compaction after turns.
    #[serde(default = "default_compaction_enabled")]
    pub enabled: bool,
    /// Context-window usage percentage that triggers compaction (1–100).
    /// Compact when context tokens exceed `context_window * threshold_pct / 100`.
    #[serde(default = "default_compaction_threshold_pct")]
    pub threshold_pct: u8,
    /// Number of recent tokens to keep after compaction.
    #[serde(default = "default_compaction_keep_recent")]
    pub keep_recent_tokens: u64,
}

impl Default for CompactionConfig {
    fn default() -> Self {
        Self {
            enabled: default_compaction_enabled(),
            threshold_pct: default_compaction_threshold_pct(),
            keep_recent_tokens: default_compaction_keep_recent(),
        }
    }
}

impl CompactionConfig {
    /// Convert to elph-agent's `CompactionSettings`.
    pub fn to_agent_settings(&self) -> elph_agent::CompactionSettings {
        elph_agent::CompactionSettings {
            enabled: self.enabled,
            reserve_tokens: elph_agent::HARNESS_DEFAULT_COMPACTION_SETTINGS.reserve_tokens,
            threshold_pct: Some(self.threshold_pct.clamp(1, 100)),
            keep_recent_tokens: self.keep_recent_tokens,
        }
    }
}

impl Settings {
    /// Built-in defaults written on first bootstrap (`Settings::ensure`).
    ///
    /// No model is pre-selected: `session.providerId` / `session.modelId` and
    /// `models.scoped` stay empty until the user picks models.
    pub fn defaults() -> Self {
        Self {
            ui: UiSettings::default(),
            session: SessionSettings {
                provider_id: None,
                model_id: None,
                agent_mode: default_agent_mode(),
                thinking_level: default_thinking_level(),
                title_model: default_title_model(),
                preferred_chat_language: default_preferred_chat_language(),
            },
            models: ModelsSettings::default(),
            provider: ProviderHttpSettings::default(),
            memory: MemorySettings::default(),
            notifications: NotificationSettings::default(),
            compaction: CompactionConfig::default(),
        }
    }

    /// Parse `provider.defaultTimeout` into milliseconds for stream options.
    pub fn provider_timeout_ms(&self) -> Option<u64> {
        parse_duration_ms(&self.provider.default_timeout)
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
        let mut value = read_settings_value(&path)?;
        migrate_settings_value(&mut value);
        serde_json::from_value(value).with_context(|| format!("parse {}", path.display()))
    }

    /// Load merged settings: serde defaults ← home ← project (project wins per field).
    pub fn load(paths: &Paths) -> Result<Self> {
        Self::ensure(paths)?;
        let mut home = read_settings_value(&paths.settings_path())?;
        migrate_settings_value(&mut home);
        let mut project = read_settings_value(&paths.project_settings_path())?;
        migrate_settings_value(&mut project);
        let mut merged = home;
        deep_merge(&mut merged, &project);
        serde_json::from_value(merged).context("parse merged settings")
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

/// Lift flat legacy keys into domain groups so older `settings.json` files still load.
fn migrate_settings_value(value: &mut Value) {
    let Some(root) = value.as_object_mut() else {
        return;
    };

    lift_into_object(
        root,
        "ui",
        &[
            "showThinking",
            "autoExpandThinking",
            "stickyScroll",
            "footerTokenDisplay",
            "coloredStatusFooter",
            "filePicker",
        ],
    );

    // Top-level scopedModelItems → models.scoped
    if let Some(scoped) = root.remove("scopedModelItems") {
        let models = root
            .entry("models".to_string())
            .or_insert_with(|| Value::Object(Map::new()));
        if let Some(obj) = models.as_object_mut() {
            obj.entry("scoped".to_string()).or_insert(scoped);
        }
    }

    // Also accept models.scopedModelItems if someone nested the old name.
    if let Some(Value::Object(models)) = root.get_mut("models")
        && let Some(scoped) = models.remove("scopedModelItems")
    {
        models.entry("scoped".to_string()).or_insert(scoped);
    }
}

fn lift_into_object(root: &mut Map<String, Value>, group: &str, keys: &[&str]) {
    let mut lifted = Map::new();
    for key in keys {
        if let Some(v) = root.remove(*key) {
            lifted.insert((*key).to_string(), v);
        }
    }
    if lifted.is_empty() {
        return;
    }
    match root.get_mut(group) {
        Some(Value::Object(existing)) => {
            for (k, v) in lifted {
                existing.entry(k).or_insert(v);
            }
        }
        _ => {
            root.insert(group.to_string(), Value::Object(lifted));
        }
    }
}

/// Deep-merge `overlay` into `base` (objects recurse; other JSON types replace).
fn deep_merge(base: &mut Value, overlay: &Value) {
    match (base, overlay) {
        (Value::Object(base_map), Value::Object(overlay_map)) => {
            for (key, overlay_value) in overlay_map {
                match base_map.get_mut(key) {
                    Some(base_value) => deep_merge(base_value, overlay_value),
                    None => {
                        base_map.insert(key.clone(), overlay_value.clone());
                    }
                }
            }
        }
        (base, overlay) => {
            *base = overlay.clone();
        }
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

fn default_embed_model() -> String {
    floppy::DEFAULT_EMBED_MODEL.to_string()
}

fn default_embed_quantized() -> bool {
    true
}

fn default_memory_top_k() -> u32 {
    5
}

fn default_memory_context_budget() -> u32 {
    3000
}

fn default_memory_min_query_length() -> u32 {
    15
}

fn default_agent_mode() -> String {
    "build".to_string()
}

fn default_thinking_level() -> String {
    "high".to_string()
}

fn default_title_model() -> String {
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

fn default_min_turn_duration_secs() -> f64 {
    5.0
}

fn default_notification_app_name() -> String {
    "Elph".to_string()
}

fn default_compaction_enabled() -> bool {
    true
}

fn default_compaction_threshold_pct() -> u8 {
    80
}

fn default_compaction_keep_recent() -> u64 {
    20_000
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
        assert_eq!(decoded.memory.embed_model, "AllMiniLML6V2");
        assert!(decoded.memory.embed_quantized);
        assert!(decoded.session.provider_id.is_none());
        assert!(decoded.session.model_id.is_none());
        assert_eq!(decoded.session.title_model, "inherit");
        assert_eq!(decoded.session.preferred_chat_language, "english");
        assert_eq!(decoded.provider.max_retries, 2);
        assert_eq!(decoded.provider.default_timeout, "120s");
        assert!(decoded.ui.show_thinking);
        assert_eq!(decoded.ui.theme, "auto");
        assert!(decoded.ui.themes.dark.is_empty());
        assert!(decoded.models.scoped.is_empty());
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
        assert!(loaded.session.provider_id.is_none());
        assert!(loaded.session.model_id.is_none());
        assert!(loaded.models.scoped.is_empty());

        let raw = std::fs::read_to_string(paths.settings_path()).expect("read");
        let value: Value = serde_json::from_str(&raw).expect("parse");
        assert!(value["session"].get("providerId").is_none());
        assert!(value["session"].get("modelId").is_none());
        assert_eq!(value["models"]["scoped"], serde_json::json!([]));
    }

    #[test]
    fn nested_shape_serializes_domain_groups() {
        let json = serde_json::to_value(Settings::defaults()).expect("ser");
        let obj = json.as_object().expect("object");
        assert!(obj.contains_key("ui"));
        assert!(obj.contains_key("session"));
        assert!(obj.contains_key("models"));
        assert!(obj.contains_key("provider"));
        assert!(obj.contains_key("memory"));
        assert!(!obj.contains_key("showThinking"));
        assert!(!obj.contains_key("scopedModelItems"));
        assert_eq!(json["ui"]["footerTokenDisplay"], "both");
        assert!(json["models"]["scoped"].as_array().expect("arr").is_empty());
    }

    #[test]
    fn migrate_flat_legacy_keys() {
        let json = r#"{
            "showThinking": false,
            "stickyScroll": false,
            "footerTokenDisplay": "count",
            "coloredStatusFooter": false,
            "autoExpandThinking": true,
            "scopedModelItems": ["opencode/big-pickle"],
            "filePicker": { "showHiddenFiles": true },
            "session": { "agentMode": "plan" },
            "provider": { "maxRetries": 4 }
        }"#;
        let mut value: Value = serde_json::from_str(json).expect("parse");
        migrate_settings_value(&mut value);
        let decoded: Settings = serde_json::from_value(value).expect("decode");
        assert!(!decoded.ui.show_thinking);
        assert!(!decoded.ui.sticky_scroll);
        assert_eq!(decoded.ui.footer_token_display, "count");
        assert!(!decoded.ui.colored_status_footer);
        assert!(decoded.ui.auto_expand_thinking);
        assert!(decoded.ui.file_picker.show_hidden_files);
        assert_eq!(decoded.models.scoped, vec!["opencode/big-pickle".to_string()]);
        assert_eq!(decoded.session.agent_mode, "plan");
        assert_eq!(decoded.provider.max_retries, 4);
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
        assert_eq!(loaded.memory.embed_model, "AllMiniLML6V2");
        // New knobs default when section/fields are missing.
        assert!(loaded.memory.enabled);
        assert!(loaded.memory.auto_recall);
        assert!(loaded.memory.auto_capture_work);
        assert!(loaded.memory.auto_capture_exploration);
        assert_eq!(loaded.memory.top_k, 5);
        assert_eq!(loaded.memory.context_budget_chars, 3000);
        assert_eq!(loaded.memory.min_query_length, 15);
    }

    #[test]
    fn memory_settings_partial_json_defaults() {
        let raw = r#"{"embedModel":"AllMiniLML6V2","embedQuantized":false}"#;
        let decoded: MemorySettings = serde_json::from_str(raw).expect("parse");
        assert!(decoded.enabled);
        assert!(decoded.auto_recall);
        assert_eq!(decoded.top_k, 5);
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
        home.session.agent_mode = "build".into();
        home.session.provider_id = Some("opencode".into());
        home.session.model_id = Some("big-pickle".into());
        Settings::save(&paths, &home).expect("save home");

        let project = serde_json::json!({
            "ui": { "showThinking": false },
            "session": { "agentMode": "plan" }
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
        assert_eq!(merged.session.agent_mode, "plan");
        assert_eq!(merged.session.provider_id.as_deref(), Some("opencode"));
        assert_eq!(merged.session.model_id.as_deref(), Some("big-pickle"));
    }

    #[test]
    fn project_can_override_session_model() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let paths = test_paths(&tmp);
        Settings::ensure(&paths).expect("ensure");

        let mut home = Settings::load_home(&paths).expect("home");
        home.session.provider_id = Some("opencode".into());
        home.session.model_id = Some("big-pickle".into());
        Settings::save(&paths, &home).expect("save home");

        let project = serde_json::json!({
            "session": {
                "providerId": "anthropic",
                "modelId": "claude-sonnet-4"
            }
        });
        std::fs::create_dir_all(paths.project_elph_dir()).expect("project dir");
        std::fs::write(
            paths.project_settings_path(),
            serde_json::to_string_pretty(&project).expect("ser"),
        )
        .expect("write project");

        let merged = Settings::load(&paths).expect("merged");
        assert_eq!(merged.session.provider_id.as_deref(), Some("anthropic"));
        assert_eq!(merged.session.model_id.as_deref(), Some("claude-sonnet-4"));
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
    fn deep_merge_replaces_arrays_and_scalars() {
        let mut base = serde_json::json!({
            "models": { "scoped": ["a/b"] },
            "ui": { "showThinking": true }
        });
        let overlay = serde_json::json!({
            "models": { "scoped": ["x/y", "z/w"] },
            "ui": { "showThinking": false }
        });
        deep_merge(&mut base, &overlay);
        assert_eq!(base["ui"]["showThinking"], false);
        assert_eq!(base["models"]["scoped"], serde_json::json!(["x/y", "z/w"]));
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
    fn load_migrates_legacy_file_on_disk() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let paths = test_paths(&tmp);
        std::fs::create_dir_all(paths.config_dir()).expect("config");
        std::fs::write(
            paths.settings_path(),
            r#"{"showThinking":false,"scopedModelItems":["opencode/big-pickle"],"session":{"agentMode":"ask"}}"#,
        )
        .expect("seed");

        let loaded = Settings::load_home(&paths).expect("load");
        assert!(!loaded.ui.show_thinking);
        assert_eq!(loaded.models.scoped, vec!["opencode/big-pickle".to_string()]);
        assert_eq!(loaded.session.agent_mode, "ask");

        Settings::save(&paths, &loaded).expect("save");
        let raw = std::fs::read_to_string(paths.settings_path()).expect("read");
        assert!(raw.contains("\"ui\""));
        assert!(raw.contains("\"models\""));
        assert!(!raw.contains("scopedModelItems"));
        assert!(!raw.contains("\"showThinking\": false") || raw.contains("\"ui\""));
        // Top-level showThinking should be gone after save.
        let value: Value = serde_json::from_str(&raw).expect("parse");
        assert!(value.get("showThinking").is_none());
        assert_eq!(value["ui"]["showThinking"], false);
        assert_eq!(value["models"]["scoped"][0], "opencode/big-pickle");
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
