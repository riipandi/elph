//! Configuration for optional TOON prompt encoding.

use serde::{Deserialize, Serialize};

const DEFAULT_MIN_BYTES: usize = 2048;
const DEFAULT_MIN_SAVINGS_RATIO: f64 = 1.0;
pub(crate) const DEFAULT_PREAMBLE: &str = "Data is in TOON format (2-space indent, arrays show length and fields).";

/// Delimiter used when encoding TOON tabular arrays.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PromptEncodingDelimiter {
    #[default]
    Comma,
    Tab,
    Pipe,
}

impl PromptEncodingDelimiter {
    pub fn as_toon_delimiter(self) -> toon_format::Delimiter {
        match self {
            Self::Comma => toon_format::Delimiter::Comma,
            Self::Tab => toon_format::Delimiter::Tab,
            Self::Pipe => toon_format::Delimiter::Pipe,
        }
    }

    pub fn from_env_str(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().as_str() {
            "comma" | "," => Some(Self::Comma),
            "tab" | "\t" => Some(Self::Tab),
            "pipe" | "|" => Some(Self::Pipe),
            _ => None,
        }
    }
}

/// When to apply TOON encoding to prompt payloads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PromptEncodingMode {
    #[default]
    Off,
    /// Encode all eligible structured payloads.
    Toon,
    /// Encode only uniform tabular JSON arrays.
    Auto,
}

/// Which tool-result surfaces TOON encoding may rewrite for the model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PromptEncodingTargets {
    pub tool_result_text: bool,
    pub structured_details: bool,
}

impl Default for PromptEncodingTargets {
    fn default() -> Self {
        Self::ALL
    }
}

impl PromptEncodingTargets {
    pub const ALL: Self = Self {
        tool_result_text: true,
        structured_details: true,
    };
}

/// Optional TOON encoding settings for agent prompt payloads.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PromptEncodingConfig {
    #[serde(deserialize_with = "deserialize_mode")]
    pub mode: PromptEncodingMode,
    #[serde(default = "default_min_bytes")]
    pub min_bytes: usize,
    /// Encode only when `toon_len <= json_len * min_savings_ratio`.
    #[serde(default = "default_min_savings_ratio")]
    pub min_savings_ratio: f64,
    pub delimiter: PromptEncodingDelimiter,
    /// Delimiter override for tabular payloads; defaults to tab per TOON LLM guide.
    #[serde(default = "default_tabular_delimiter")]
    pub tabular_delimiter: Option<PromptEncodingDelimiter>,
    pub targets: PromptEncodingTargets,
    #[serde(default = "default_preamble")]
    pub preamble: Option<String>,
}

impl Default for PromptEncodingConfig {
    fn default() -> Self {
        Self {
            mode: PromptEncodingMode::Off,
            min_bytes: default_min_bytes(),
            min_savings_ratio: default_min_savings_ratio(),
            delimiter: PromptEncodingDelimiter::Comma,
            tabular_delimiter: default_tabular_delimiter(),
            targets: PromptEncodingTargets::ALL,
            preamble: default_preamble(),
        }
    }
}

impl PromptEncodingConfig {
    pub fn is_enabled(&self) -> bool {
        !matches!(self.mode, PromptEncodingMode::Off)
    }

    pub(crate) fn delimiter_for_value(&self, value: &serde_json::Value) -> PromptEncodingDelimiter {
        if super::heuristic::is_tabular_json(value) {
            self.tabular_delimiter.unwrap_or(PromptEncodingDelimiter::Tab)
        } else {
            self.delimiter
        }
    }

    /// Resolve from environment variables. Unknown values fall back safely.
    pub fn from_env() -> Self {
        let mut config = Self {
            mode: parse_mode_from_env(),
            ..Self::default()
        };
        if let Some(min_bytes) = parse_usize_env("ELPH_PROMPT_ENCODING_MIN_BYTES") {
            config.min_bytes = min_bytes;
        }
        if let Some(delimiter) = std::env::var("ELPH_PROMPT_ENCODING_DELIMITER")
            .ok()
            .and_then(|v| PromptEncodingDelimiter::from_env_str(&v))
        {
            config.delimiter = delimiter;
        }
        if let Some(tabular) = std::env::var("ELPH_PROMPT_ENCODING_TABULAR_DELIMITER")
            .ok()
            .and_then(|v| PromptEncodingDelimiter::from_env_str(&v))
        {
            config.tabular_delimiter = Some(tabular);
        }
        config
    }
}

fn parse_mode_from_env() -> PromptEncodingMode {
    match std::env::var("ELPH_PROMPT_ENCODING")
        .ok()
        .map(|v| v.to_ascii_lowercase())
        .as_deref()
    {
        Some("toon") => PromptEncodingMode::Toon,
        Some("auto") => PromptEncodingMode::Auto,
        _ => PromptEncodingMode::Off,
    }
}

fn parse_usize_env(name: &str) -> Option<usize> {
    std::env::var(name).ok()?.parse().ok()
}

fn default_min_bytes() -> usize {
    DEFAULT_MIN_BYTES
}

fn default_min_savings_ratio() -> f64 {
    DEFAULT_MIN_SAVINGS_RATIO
}

fn default_tabular_delimiter() -> Option<PromptEncodingDelimiter> {
    Some(PromptEncodingDelimiter::Tab)
}

fn default_preamble() -> Option<String> {
    Some(DEFAULT_PREAMBLE.to_string())
}

/// Lenient mode parsing for settings.json: unknown values fall back to `Off`
/// (mirrors [`parse_mode_from_env`]).
fn deserialize_mode<'de, D>(deserializer: D) -> Result<PromptEncodingMode, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw = String::deserialize(deserializer)?;
    Ok(match raw.to_ascii_lowercase().as_str() {
        "toon" => PromptEncodingMode::Toon,
        "auto" => PromptEncodingMode::Auto,
        _ => PromptEncodingMode::Off,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delimiter_parses_aliases() {
        assert_eq!(PromptEncodingDelimiter::from_env_str("tab"), Some(PromptEncodingDelimiter::Tab));
        assert_eq!(PromptEncodingDelimiter::from_env_str("|"), Some(PromptEncodingDelimiter::Pipe));
        assert!(PromptEncodingDelimiter::from_env_str("space").is_none());
    }

    #[test]
    fn tabular_delimiter_defaults_to_tab() {
        let config = PromptEncodingConfig::default();
        assert_eq!(config.tabular_delimiter, Some(PromptEncodingDelimiter::Tab));
    }

    #[test]
    fn serde_round_trip_matches_defaults() {
        let config = PromptEncodingConfig::default();
        let json = serde_json::to_value(&config).expect("serialize");
        assert_eq!(json["mode"], "off");
        assert_eq!(json["minBytes"], 2048);
        assert_eq!(json["minSavingsRatio"], 1.0);
        assert_eq!(json["delimiter"], "comma");
        assert_eq!(json["tabularDelimiter"], "tab");
        assert_eq!(json["targets"]["toolResultText"], true);
        assert_eq!(json["targets"]["structuredDetails"], true);
        let decoded: PromptEncodingConfig = serde_json::from_value(json).expect("deserialize");
        assert_eq!(config, decoded);
    }

    #[test]
    fn serde_partial_object_uses_field_defaults() {
        let decoded: PromptEncodingConfig =
            serde_json::from_value(serde_json::json!({ "mode": "auto" })).expect("deserialize partial");
        let mut expected = PromptEncodingConfig::default();
        expected.mode = PromptEncodingMode::Auto;
        assert_eq!(decoded, expected);
    }

    #[test]
    fn serde_unknown_mode_falls_back_to_off() {
        let decoded: PromptEncodingConfig =
            serde_json::from_value(serde_json::json!({ "mode": "bogus" })).expect("deserialize");
        assert_eq!(decoded.mode, PromptEncodingMode::Off);
    }
}
