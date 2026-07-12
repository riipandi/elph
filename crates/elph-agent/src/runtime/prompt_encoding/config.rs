//! Configuration for optional TOON prompt encoding.

const DEFAULT_MIN_BYTES: usize = 2048;
const DEFAULT_MIN_SAVINGS_RATIO: f64 = 1.0;
pub(crate) const DEFAULT_PREAMBLE: &str = "Data is in TOON format (2-space indent, arrays show length and fields).";

/// Delimiter used when encoding TOON tabular arrays.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PromptEncodingMode {
    #[default]
    Off,
    /// Encode all eligible structured payloads.
    Toon,
    /// Encode only uniform tabular JSON arrays.
    Auto,
}

/// Which tool-result surfaces TOON encoding may rewrite for the model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PromptEncodingTargets {
    pub tool_result_text: bool,
    pub structured_details: bool,
}

impl PromptEncodingTargets {
    pub const ALL: Self = Self {
        tool_result_text: true,
        structured_details: true,
    };
}

/// Optional TOON encoding settings for agent prompt payloads.
#[derive(Debug, Clone, PartialEq)]
pub struct PromptEncodingConfig {
    pub mode: PromptEncodingMode,
    pub min_bytes: usize,
    /// Encode only when `toon_len <= json_len * min_savings_ratio`.
    pub min_savings_ratio: f64,
    pub delimiter: PromptEncodingDelimiter,
    /// Delimiter override for tabular payloads; defaults to tab per TOON LLM guide.
    pub tabular_delimiter: Option<PromptEncodingDelimiter>,
    pub targets: PromptEncodingTargets,
    pub preamble: Option<String>,
}

impl Default for PromptEncodingConfig {
    fn default() -> Self {
        Self {
            mode: PromptEncodingMode::Off,
            min_bytes: DEFAULT_MIN_BYTES,
            min_savings_ratio: DEFAULT_MIN_SAVINGS_RATIO,
            delimiter: PromptEncodingDelimiter::Comma,
            tabular_delimiter: Some(PromptEncodingDelimiter::Tab),
            targets: PromptEncodingTargets::ALL,
            preamble: Some(DEFAULT_PREAMBLE.to_string()),
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
}
