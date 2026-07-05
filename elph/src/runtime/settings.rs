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
                agent_mode: default_agent_mode(),
                thinking_level: default_thinking_level(),
            },
            database: DatabaseSettings::default(),
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
    }

    #[test]
    fn ensure_writes_only_when_missing() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let paths = Paths::from_dirs(
            tmp.path().to_path_buf(),
            tmp.path().join("data"),
            tmp.path().join("repo"),
        );

        Settings::ensure(&paths).expect("first ensure");
        assert!(paths.settings_path().exists());

        let before = std::fs::read_to_string(paths.settings_path()).expect("read settings");
        Settings::ensure(&paths).expect("second ensure");
        let after = std::fs::read_to_string(paths.settings_path()).expect("read settings");
        assert_eq!(before, after);
    }
}
