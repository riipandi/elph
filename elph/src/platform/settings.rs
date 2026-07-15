use elph_agent::write_json_file;
use serde::{Deserialize, Serialize};

use super::paths::{AppPaths, Paths};
use anyhow::Result;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    #[serde(default = "default_sync_interval")]
    pub sync_interval: String,
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default = "default_true")]
    pub show_thinking: bool,
    #[serde(default = "default_false")]
    pub auto_expand_thinking: bool,
    #[serde(default = "default_false")]
    pub use_raw_paste: bool,
    #[serde(default = "default_true")]
    pub sticky_scroll: bool,
    #[serde(default = "default_response_language")]
    pub prefered_response_language: String,
    #[serde(default)]
    pub session: SessionSettings,
    #[serde(default)]
    pub database: DatabaseSettings,
    #[serde(default)]
    pub memory: MemorySettings,
    #[serde(default = "default_true")]
    pub auto_compact_context: bool,
    #[serde(default = "default_compact_limit")]
    pub auto_compact_limit: u8,
    #[serde(default = "default_footer_token_display")]
    pub footer_token_display: String,
}

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
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseSettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MemorySettings {
    /// Embedding model catalog name or Hugging Face repo id (see `elph_core::floppy::resolve_embedding_model`).
    #[serde(default = "default_embed_model")]
    pub embed_model: String,
    /// Prefer quantized model weights when a `*Q` variant exists (default: true).
    #[serde(default = "default_embed_quantized")]
    pub embed_quantized: bool,
}

impl Default for MemorySettings {
    fn default() -> Self {
        Self {
            embed_model: default_embed_model(),
            embed_quantized: default_embed_quantized(),
        }
    }
}

impl Settings {
    pub fn defaults() -> Self {
        Self {
            sync_interval: default_sync_interval(),
            theme: default_theme(),
            show_thinking: true,
            auto_expand_thinking: false,
            use_raw_paste: false,
            sticky_scroll: true,
            prefered_response_language: default_response_language(),
            session: SessionSettings {
                provider_id: Some(default_provider_id()),
                model_id: Some(default_model_id()),
                agent_mode: default_agent_mode(),
                thinking_level: default_thinking_level(),
            },
            database: DatabaseSettings::default(),
            memory: MemorySettings::default(),
            auto_compact_context: true,
            auto_compact_limit: default_compact_limit(),
            footer_token_display: default_footer_token_display(),
        }
    }

    pub fn ensure(paths: &Paths) -> Result<()> {
        let path = paths.settings_path();
        if path.exists() {
            return Ok(());
        }

        write_json_file(&path, &Self::defaults())?;
        Ok(())
    }

    /// Load settings from disk, falling back to defaults for missing fields.
    pub fn load(paths: &Paths) -> Result<Self> {
        Self::ensure(paths)?;
        let raw = std::fs::read_to_string(paths.settings_path())?;
        Ok(serde_json::from_str(&raw)?)
    }

    /// Persist settings to disk.
    pub fn save(paths: &Paths, settings: &Self) -> Result<()> {
        write_json_file(&paths.settings_path(), settings)
    }
}

fn default_embed_model() -> String {
    elph_core::floppy::DEFAULT_EMBED_MODEL.to_string()
}

fn default_embed_quantized() -> bool {
    true
}

fn default_sync_interval() -> String {
    "24h".to_string()
}

fn default_theme() -> String {
    "auto".to_string()
}

fn default_response_language() -> String {
    "inherit".to_string()
}

fn default_provider_id() -> String {
    crate::agent::DEFAULT_PROVIDER.to_string()
}

fn default_model_id() -> String {
    crate::agent::DEFAULT_MODEL_ID.to_string()
}

fn default_agent_mode() -> String {
    "build".to_string()
}

fn default_thinking_level() -> String {
    "high".to_string()
}

fn default_compact_limit() -> u8 {
    80
}

fn default_footer_token_display() -> String {
    "both".to_string()
}

fn default_true() -> bool {
    true
}

fn default_false() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_settings_round_trip() {
        let settings = Settings::defaults();
        let json = serde_json::to_string_pretty(&settings).expect("serialize");
        let decoded: Settings = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(settings, decoded);
        assert_eq!(decoded.memory.embed_model, "AllMiniLML6V2");
        assert!(decoded.memory.embed_quantized);
        assert_eq!(decoded.session.provider_id.as_deref(), Some("opencode"));
        assert_eq!(decoded.session.model_id.as_deref(), Some("big-pickle"));
    }

    #[test]
    fn load_merges_missing_memory_section() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let paths = Paths::from_dirs(tmp.path().to_path_buf(), tmp.path().join("data"), tmp.path().join("repo"));
        Settings::ensure(&paths).expect("ensure");
        let loaded = Settings::load(&paths).expect("load");
        assert_eq!(loaded.memory.embed_model, "AllMiniLML6V2");
    }

    #[test]
    fn ensure_writes_only_when_missing() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let paths = Paths::from_dirs(tmp.path().to_path_buf(), tmp.path().join("data"), tmp.path().join("repo"));

        Settings::ensure(&paths).expect("first ensure");
        assert!(paths.settings_path().exists());

        let before = std::fs::read_to_string(paths.settings_path()).expect("read settings");
        Settings::ensure(&paths).expect("second ensure");
        let after = std::fs::read_to_string(paths.settings_path()).expect("read settings");
        assert_eq!(before, after);
    }
}
