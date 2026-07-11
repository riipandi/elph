//! Configuration for optional TOON prompt encoding.

const DEFAULT_MIN_BYTES: usize = 2048;
const DEFAULT_PREAMBLE: &str = "Structured data below uses TOON format.";

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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptEncodingConfig {
    pub mode: PromptEncodingMode,
    pub min_bytes: usize,
    pub targets: PromptEncodingTargets,
    pub preamble: Option<String>,
}

impl Default for PromptEncodingConfig {
    fn default() -> Self {
        Self {
            mode: PromptEncodingMode::Off,
            min_bytes: DEFAULT_MIN_BYTES,
            targets: PromptEncodingTargets::ALL,
            preamble: Some(DEFAULT_PREAMBLE.to_string()),
        }
    }
}

impl PromptEncodingConfig {
    pub fn is_enabled(&self) -> bool {
        !matches!(self.mode, PromptEncodingMode::Off)
    }

    /// Resolve from `ELPH_PROMPT_ENCODING` (`off`, `toon`, `auto`). Unknown values → `Off`.
    pub fn from_env() -> Self {
        match std::env::var("ELPH_PROMPT_ENCODING")
            .ok()
            .map(|v| v.to_ascii_lowercase())
            .as_deref()
        {
            Some("toon") => Self {
                mode: PromptEncodingMode::Toon,
                ..Self::default()
            },
            Some("auto") => Self {
                mode: PromptEncodingMode::Auto,
                ..Self::default()
            },
            _ => Self::default(),
        }
    }
}
