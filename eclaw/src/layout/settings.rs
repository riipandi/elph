use elph_agent::write_json_file;
use serde::{Deserialize, Serialize};

use super::paths::{AppPaths, Paths};
use anyhow::Result;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default = "default_persona")]
    pub default_persona: String,
    #[serde(default = "default_response_language")]
    pub prefered_response_language: String,
}

impl Settings {
    pub fn defaults() -> Self {
        Self {
            theme: default_theme(),
            default_persona: default_persona(),
            prefered_response_language: default_response_language(),
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

fn default_theme() -> String {
    "auto".to_string()
}

fn default_persona() -> String {
    "default".to_string()
}

fn default_response_language() -> String {
    "inherit".to_string()
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
}
